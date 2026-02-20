//! PersistentVolume and PersistentVolumeClaim binding controller
//!
//! Binds PVCs to available PVs based on access modes, capacity, and selectors.

use std::sync::Arc;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{
    LabelSelector, PersistentVolume, PersistentVolumeClaim,
    PersistentVolumePhase, PersistentVolumeClaimPhase,
    PersistentVolumeAccessMode,
};
use tracing::{debug, info, warn};

use crate::{Controller, ControllerResult};

pub struct PvPvcController {
    storage: Arc<SledBackend>,
}

impl PvPvcController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Check if a PV can satisfy a PVC's requirements
    fn pv_matches_pvc(pv: &PersistentVolume, pvc: &PersistentVolumeClaim) -> bool {
        let pv_spec = match &pv.spec {
            Some(s) => s,
            None => return false,
        };
        let pvc_spec = match &pvc.spec {
            Some(s) => s,
            None => return false,
        };

        // Check storage class
        if pv_spec.storage_class_name != pvc_spec.storage_class_name {
            return false;
        }

        // Check access modes - PV must support all PVC's requested access modes
        for pvc_mode in &pvc_spec.access_modes {
            if !pv_spec.access_modes.contains(pvc_mode) {
                return false;
            }
        }

        // Check volume mode
        if pv_spec.volume_mode != pvc_spec.volume_mode {
            return false;
        }

        // Check capacity
        if let Some(resources) = &pvc_spec.resources {
            if let Some(requested) = resources.requests.get("storage") {
                if let Some(pv_capacity) = pv_spec.capacity.get("storage") {
                    // Simple string comparison - in production, parse quantities
                    if !Self::capacity_sufficient(pv_capacity, requested) {
                        return false;
                    }
                }
            }
        }

        // Check selector if present
        if let Some(selector) = &pvc_spec.selector {
            if !Self::labels_match_selector(&pv.metadata.labels, selector) {
                return false;
            }
        }

        true
    }

    /// Parse and compare storage quantities (simplified)
    fn capacity_sufficient(pv_capacity: &str, requested: &str) -> bool {
        // Parse quantities like "10Gi", "100Mi", etc.
        fn parse_quantity(s: &str) -> Option<u64> {
            let s = s.trim();
            if s.ends_with("Ti") {
                s[..s.len() - 2].parse::<u64>().ok().map(|n| n * 1024 * 1024 * 1024 * 1024)
            } else if s.ends_with("Gi") {
                s[..s.len() - 2].parse::<u64>().ok().map(|n| n * 1024 * 1024 * 1024)
            } else if s.ends_with("Mi") {
                s[..s.len() - 2].parse::<u64>().ok().map(|n| n * 1024 * 1024)
            } else if s.ends_with("Ki") {
                s[..s.len() - 2].parse::<u64>().ok().map(|n| n * 1024)
            } else {
                s.parse::<u64>().ok()
            }
        }

        match (parse_quantity(pv_capacity), parse_quantity(requested)) {
            (Some(pv_bytes), Some(req_bytes)) => pv_bytes >= req_bytes,
            _ => false,
        }
    }

    /// Check if labels match a selector
    fn labels_match_selector(
        labels: &std::collections::BTreeMap<String, String>,
        selector: &LabelSelector,
    ) -> bool {
        // Check match_labels
        for (k, v) in &selector.match_labels {
            if labels.get(k) != Some(v) {
                return false;
            }
        }

        // Check match_expressions (simplified)
        for expr in &selector.match_expressions {
            let label_value = labels.get(&expr.key);
            let matches = match expr.operator.as_str() {
                "In" => label_value.map(|v| expr.values.contains(v)).unwrap_or(false),
                "NotIn" => label_value.map(|v| !expr.values.contains(v)).unwrap_or(true),
                "Exists" => label_value.is_some(),
                "DoesNotExist" => label_value.is_none(),
                _ => false,
            };
            if !matches {
                return false;
            }
        }

        true
    }
}

#[async_trait]
impl Controller for PvPvcController {
    fn name(&self) -> &str {
        "pv-pvc-binder"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let pv_store = ResourceStore::<PersistentVolume>::new(self.storage.clone());
        let pvc_store = ResourceStore::<PersistentVolumeClaim>::new(self.storage.clone());

        // Get all PVs and PVCs
        let pvs = pv_store.list(None).await?;
        let pvcs = pvc_store.list(None).await?;

        // Find unbound PVCs
        let pending_pvcs: Vec<&PersistentVolumeClaim> = pvcs
            .iter()
            .filter(|pvc| {
                let phase = pvc
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.as_ref());
                matches!(phase, None | Some(PersistentVolumeClaimPhase::Pending))
                    && pvc.spec.as_ref().and_then(|s| s.volume_name.as_ref()).is_none()
            })
            .collect();

        // Find available PVs
        let available_pvs: Vec<&PersistentVolume> = pvs
            .iter()
            .filter(|pv| {
                let phase = pv
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.as_ref());
                matches!(phase, None | Some(PersistentVolumePhase::Available))
                    && pv.spec.as_ref().and_then(|s| s.claim_ref.as_ref()).is_none()
            })
            .collect();

        debug!(
            "PV/PVC controller: {} pending PVCs, {} available PVs",
            pending_pvcs.len(),
            available_pvs.len()
        );

        // Try to bind each pending PVC to an available PV
        for pvc in pending_pvcs {
            let namespace = pvc.metadata.namespace.as_deref().unwrap_or("default");

            // Find a matching PV
            let matching_pv = available_pvs
                .iter()
                .find(|pv| Self::pv_matches_pvc(pv, pvc));

            if let Some(pv) = matching_pv {
                info!(
                    "Binding PVC {}/{} to PV {}",
                    namespace, pvc.metadata.name, pv.metadata.name
                );

                // Update PVC with volume name and status
                let mut updated_pvc = (*pvc).clone();
                if let Some(spec) = &mut updated_pvc.spec {
                    spec.volume_name = Some(pv.metadata.name.clone());
                }
                updated_pvc.status = Some(k1s_types::PersistentVolumeClaimStatus {
                    phase: Some(PersistentVolumeClaimPhase::Bound),
                    access_modes: pv
                        .spec
                        .as_ref()
                        .map(|s| s.access_modes.clone())
                        .unwrap_or_default(),
                    capacity: pv
                        .spec
                        .as_ref()
                        .map(|s| s.capacity.clone())
                        .unwrap_or_default(),
                    conditions: vec![],
                });

                if let Err(e) = pvc_store.update(updated_pvc).await {
                    warn!(
                        "Failed to update PVC {}/{}: {}",
                        namespace, pvc.metadata.name, e
                    );
                    continue;
                }

                // Update PV with claim reference and status
                let mut updated_pv = (*pv).clone();
                if let Some(spec) = &mut updated_pv.spec {
                    // Get the existing claim_ref and set fields, or create new default
                    let mut claim_ref = spec.claim_ref.take().unwrap_or_default();
                    claim_ref.api_version = Some("v1".to_string());
                    claim_ref.kind = Some("PersistentVolumeClaim".to_string());
                    claim_ref.name = Some(pvc.metadata.name.clone());
                    claim_ref.namespace = Some(namespace.to_string());
                    claim_ref.uid = Some(pvc.metadata.uid.clone());
                    claim_ref.resource_version = Some(pvc.metadata.resource_version.clone());
                    spec.claim_ref = Some(claim_ref);
                }
                updated_pv.status = Some(k1s_types::PersistentVolumeStatus {
                    phase: Some(PersistentVolumePhase::Bound),
                    message: None,
                    reason: None,
                });

                if let Err(e) = pv_store.update(updated_pv).await {
                    warn!("Failed to update PV {}: {}", pv.metadata.name, e);
                }
            }
        }

        // Handle released PVs based on reclaim policy
        let released_pvs: Vec<&PersistentVolume> = pvs
            .iter()
            .filter(|pv| {
                matches!(
                    pv.status.as_ref().and_then(|s| s.phase.as_ref()),
                    Some(PersistentVolumePhase::Released)
                )
            })
            .collect();

        for pv in released_pvs {
            let reclaim_policy = pv
                .spec
                .as_ref()
                .and_then(|s| s.persistent_volume_reclaim_policy.as_ref());

            match reclaim_policy {
                Some(k1s_types::PersistentVolumeReclaimPolicy::Delete) => {
                    info!("Deleting released PV {} (Delete policy)", pv.metadata.name);
                    if let Err(e) = pv_store.delete(None, &pv.metadata.name).await {
                        warn!("Failed to delete PV {}: {}", pv.metadata.name, e);
                    }
                }
                Some(k1s_types::PersistentVolumeReclaimPolicy::Recycle) => {
                    // Mark as Available again (simplified - real recycling would clear data)
                    info!("Recycling released PV {}", pv.metadata.name);
                    let mut updated_pv = pv.clone();
                    if let Some(spec) = &mut updated_pv.spec {
                        spec.claim_ref = None;
                    }
                    updated_pv.status = Some(k1s_types::PersistentVolumeStatus {
                        phase: Some(PersistentVolumePhase::Available),
                        message: None,
                        reason: None,
                    });
                    if let Err(e) = pv_store.update(updated_pv).await {
                        warn!("Failed to recycle PV {}: {}", pv.metadata.name, e);
                    }
                }
                _ => {
                    // Retain policy - do nothing
                    debug!("PV {} retained (Retain policy)", pv.metadata.name);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capacity_sufficient() {
        assert!(PvPvcController::capacity_sufficient("10Gi", "5Gi"));
        assert!(PvPvcController::capacity_sufficient("10Gi", "10Gi"));
        assert!(!PvPvcController::capacity_sufficient("5Gi", "10Gi"));
        assert!(PvPvcController::capacity_sufficient("1Ti", "500Gi"));
        assert!(PvPvcController::capacity_sufficient("1024Mi", "1Gi"));
    }
}
