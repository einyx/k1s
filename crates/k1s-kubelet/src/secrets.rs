//! ConfigMap and Secret resolution for pod containers
//!
//! Resolves ConfigMap and Secret references from pods to inject
//! as environment variables or volume mounts.

use std::collections::HashMap;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine};
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{ConfigMap, Container, EnvFromSource, EnvVar, Pod, Secret, Volume};
use tracing::{debug, warn};

use crate::{KubeletError, KubeletResult};

/// Resolved environment variables and volume data for a container
#[derive(Debug, Clone, Default)]
pub struct ResolvedContainerEnv {
    /// Environment variables as KEY=VALUE pairs
    pub env_vars: Vec<String>,
    /// Volume mount data: map of mount_path -> file contents
    pub volume_data: HashMap<String, HashMap<String, Vec<u8>>>,
}

/// Resolver for ConfigMaps and Secrets
pub struct SecretResolver {
    storage: Arc<SledBackend>,
}

impl SecretResolver {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Resolve all environment variables and volume mounts for a container
    pub async fn resolve_container_env(
        &self,
        pod: &Pod,
        container: &Container,
    ) -> KubeletResult<ResolvedContainerEnv> {
        let namespace = pod.metadata.effective_namespace();
        let mut result = ResolvedContainerEnv::default();

        // Resolve envFrom sources
        for env_from in &container.env_from {
            self.resolve_env_from(namespace, env_from, &mut result.env_vars)
                .await?;
        }

        // Resolve individual env vars
        for env_var in &container.env {
            if let Some(env_str) = self.resolve_env_var(namespace, env_var).await? {
                result.env_vars.push(env_str);
            }
        }

        // Resolve volume mounts
        if let Some(spec) = &pod.spec {
            for volume_mount in &container.volume_mounts {
                // Find the corresponding volume
                if let Some(volume) = spec.volumes.iter().find(|v| v.name == volume_mount.name) {
                    if let Some(data) = self.resolve_volume(namespace, volume).await? {
                        result.volume_data.insert(volume_mount.mount_path.clone(), data);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Resolve envFrom source (all keys from ConfigMap or Secret)
    async fn resolve_env_from(
        &self,
        namespace: &str,
        env_from: &EnvFromSource,
        env_vars: &mut Vec<String>,
    ) -> KubeletResult<()> {
        let prefix = env_from.prefix.as_deref().unwrap_or("");

        // Handle ConfigMap reference
        if let Some(cm_ref) = &env_from.config_map_ref {
            match self.get_configmap(namespace, &cm_ref.name).await {
                Ok(Some(cm)) => {
                    for (key, value) in &cm.data {
                        env_vars.push(format!("{prefix}{key}={value}"));
                    }
                }
                Ok(None) => {
                    if cm_ref.optional != Some(true) {
                        return Err(KubeletError::Pod(format!(
                            "ConfigMap {} not found",
                            cm_ref.name
                        )));
                    }
                    debug!("Optional ConfigMap {} not found", cm_ref.name);
                }
                Err(e) => {
                    if cm_ref.optional != Some(true) {
                        return Err(e);
                    }
                }
            }
        }

        // Handle Secret reference
        if let Some(secret_ref) = &env_from.secret_ref {
            match self.get_secret(namespace, &secret_ref.name).await {
                Ok(Some(secret)) => {
                    // Decode base64 data
                    for (key, value) in &secret.data {
                        if let Ok(decoded) = STANDARD.decode(value) {
                            if let Ok(str_value) = String::from_utf8(decoded) {
                                env_vars.push(format!("{prefix}{key}={str_value}"));
                            }
                        }
                    }
                    // stringData is already plain text
                    for (key, value) in &secret.string_data {
                        env_vars.push(format!("{prefix}{key}={value}"));
                    }
                }
                Ok(None) => {
                    if secret_ref.optional != Some(true) {
                        return Err(KubeletError::Pod(format!(
                            "Secret {} not found",
                            secret_ref.name
                        )));
                    }
                    debug!("Optional Secret {} not found", secret_ref.name);
                }
                Err(e) => {
                    if secret_ref.optional != Some(true) {
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolve a single environment variable
    async fn resolve_env_var(
        &self,
        namespace: &str,
        env_var: &EnvVar,
    ) -> KubeletResult<Option<String>> {
        // Direct value
        if let Some(value) = &env_var.value {
            return Ok(Some(format!("{}={}", env_var.name, value)));
        }

        // Value from source
        if let Some(value_from) = &env_var.value_from {
            // ConfigMap key reference
            if let Some(cm_ref) = &value_from.config_map_key_ref {
                match self.get_configmap(namespace, &cm_ref.name).await {
                    Ok(Some(cm)) => {
                        if let Some(value) = cm.data.get(&cm_ref.key) {
                            return Ok(Some(format!("{}={}", env_var.name, value)));
                        } else if cm_ref.optional != Some(true) {
                            return Err(KubeletError::Pod(format!(
                                "Key {} not found in ConfigMap {}",
                                cm_ref.key, cm_ref.name
                            )));
                        }
                    }
                    Ok(None) => {
                        if cm_ref.optional != Some(true) {
                            return Err(KubeletError::Pod(format!(
                                "ConfigMap {} not found",
                                cm_ref.name
                            )));
                        }
                    }
                    Err(e) => {
                        if cm_ref.optional != Some(true) {
                            return Err(e);
                        }
                    }
                }
            }

            // Secret key reference
            if let Some(secret_ref) = &value_from.secret_key_ref {
                match self.get_secret(namespace, &secret_ref.name).await {
                    Ok(Some(secret)) => {
                        // Try stringData first, then data (base64)
                        if let Some(value) = secret.string_data.get(&secret_ref.key) {
                            return Ok(Some(format!("{}={}", env_var.name, value)));
                        } else if let Some(value) = secret.data.get(&secret_ref.key) {
                            if let Ok(decoded) = STANDARD.decode(value) {
                                if let Ok(str_value) = String::from_utf8(decoded) {
                                    return Ok(Some(format!("{}={}", env_var.name, str_value)));
                                }
                            }
                        } else if secret_ref.optional != Some(true) {
                            return Err(KubeletError::Pod(format!(
                                "Key {} not found in Secret {}",
                                secret_ref.key, secret_ref.name
                            )));
                        }
                    }
                    Ok(None) => {
                        if secret_ref.optional != Some(true) {
                            return Err(KubeletError::Pod(format!(
                                "Secret {} not found",
                                secret_ref.name
                            )));
                        }
                    }
                    Err(e) => {
                        if secret_ref.optional != Some(true) {
                            return Err(e);
                        }
                    }
                }
            }

            // Field reference (downward API)
            if let Some(field_ref) = &value_from.field_ref {
                if let Some(value) = self.resolve_field_ref(&field_ref.field_path, namespace) {
                    return Ok(Some(format!("{}={}", env_var.name, value)));
                }
            }
        }

        Ok(None)
    }

    /// Resolve volume data from ConfigMap or Secret
    async fn resolve_volume(
        &self,
        namespace: &str,
        volume: &Volume,
    ) -> KubeletResult<Option<HashMap<String, Vec<u8>>>> {
        // ConfigMap volume
        if let Some(cm_source) = &volume.source.config_map {
            match self.get_configmap(namespace, &cm_source.name).await {
                Ok(Some(cm)) => {
                    let mut data = HashMap::new();

                    if cm_source.items.is_empty() {
                        // Mount all keys
                        for (key, value) in &cm.data {
                            data.insert(key.clone(), value.as_bytes().to_vec());
                        }
                    } else {
                        // Mount specific keys with path mappings
                        for item in &cm_source.items {
                            if let Some(value) = cm.data.get(&item.key) {
                                data.insert(item.path.clone(), value.as_bytes().to_vec());
                            }
                        }
                    }

                    return Ok(Some(data));
                }
                Ok(None) => {
                    if cm_source.optional != Some(true) {
                        return Err(KubeletError::Pod(format!(
                            "ConfigMap {} not found",
                            cm_source.name
                        )));
                    }
                }
                Err(e) => {
                    if cm_source.optional != Some(true) {
                        return Err(e);
                    }
                }
            }
        }

        // Secret volume
        if let Some(secret_source) = &volume.source.secret {
            match self.get_secret(namespace, &secret_source.secret_name).await {
                Ok(Some(secret)) => {
                    let mut data = HashMap::new();

                    if secret_source.items.is_empty() {
                        // Mount all keys
                        for (key, value) in &secret.data {
                            if let Ok(decoded) = STANDARD.decode(value) {
                                data.insert(key.clone(), decoded);
                            }
                        }
                        for (key, value) in &secret.string_data {
                            data.insert(key.clone(), value.as_bytes().to_vec());
                        }
                    } else {
                        // Mount specific keys with path mappings
                        for item in &secret_source.items {
                            if let Some(value) = secret.data.get(&item.key) {
                                if let Ok(decoded) = STANDARD.decode(value) {
                                    data.insert(item.path.clone(), decoded);
                                }
                            } else if let Some(value) = secret.string_data.get(&item.key) {
                                data.insert(item.path.clone(), value.as_bytes().to_vec());
                            }
                        }
                    }

                    return Ok(Some(data));
                }
                Ok(None) => {
                    if secret_source.optional != Some(true) {
                        return Err(KubeletError::Pod(format!(
                            "Secret {} not found",
                            secret_source.secret_name
                        )));
                    }
                }
                Err(e) => {
                    if secret_source.optional != Some(true) {
                        return Err(e);
                    }
                }
            }
        }

        Ok(None)
    }

    /// Resolve downward API field reference
    fn resolve_field_ref(&self, field_path: &str, namespace: &str) -> Option<String> {
        match field_path {
            "metadata.namespace" => Some(namespace.to_string()),
            _ => {
                warn!("Unsupported field reference: {}", field_path);
                None
            }
        }
    }

    async fn get_configmap(&self, namespace: &str, name: &str) -> KubeletResult<Option<ConfigMap>> {
        let store = ResourceStore::<ConfigMap>::new(self.storage.clone());
        store
            .get(Some(namespace), name)
            .await
            .map_err(|e| KubeletError::Pod(format!("Failed to get ConfigMap {name}: {e}")))
    }

    async fn get_secret(&self, namespace: &str, name: &str) -> KubeletResult<Option<Secret>> {
        let store = ResourceStore::<Secret>::new(self.storage.clone());
        store
            .get(Some(namespace), name)
            .await
            .map_err(|e| KubeletError::Pod(format!("Failed to get Secret {name}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k1s_storage::backend::ResourceStore;
    use k1s_types::{EnvFromSource, EnvVar, EnvVarSource, ConfigMapEnvSource};
    use k1s_types::{SecretKeySelector, PodSpec};

    #[test]
    fn test_resolved_container_env_default() {
        let env = ResolvedContainerEnv::default();
        assert!(env.env_vars.is_empty());
        assert!(env.volume_data.is_empty());
    }

    #[test]
    fn test_field_ref_resolution() {
        let resolver = SecretResolver {
            storage: Arc::new(SledBackend::in_memory().unwrap()),
        };

        assert_eq!(
            resolver.resolve_field_ref("metadata.namespace", "test-ns"),
            Some("test-ns".to_string())
        );
        assert_eq!(
            resolver.resolve_field_ref("unknown.field", "test-ns"),
            None
        );
    }

    #[tokio::test]
    async fn test_resolve_configmap_env() {
        let storage = Arc::new(SledBackend::in_memory().unwrap());
        let resolver = SecretResolver::new(storage.clone());

        // Create a ConfigMap
        let cm_store = ResourceStore::<ConfigMap>::new(storage.clone());
        let mut cm = ConfigMap::namespaced("my-config", "default");
        cm.data.insert("DB_HOST".to_string(), "localhost".to_string());
        cm.data.insert("DB_PORT".to_string(), "5432".to_string());
        cm_store.create(cm).await.unwrap();

        // Create a pod with envFrom referencing the ConfigMap
        let spec = PodSpec {
            containers: vec![Container {
                name: "test".to_string(),
                image: "nginx".to_string(),
                env_from: vec![EnvFromSource {
                    config_map_ref: Some(ConfigMapEnvSource {
                        name: "my-config".to_string(),
                        optional: None,
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let pod = Pod::namespaced("test-pod", "default", spec);

        let container = &pod.spec.as_ref().unwrap().containers[0];
        let resolved = resolver.resolve_container_env(&pod, container).await.unwrap();

        assert!(resolved.env_vars.contains(&"DB_HOST=localhost".to_string()));
        assert!(resolved.env_vars.contains(&"DB_PORT=5432".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_secret_env() {
        let storage = Arc::new(SledBackend::in_memory().unwrap());
        let resolver = SecretResolver::new(storage.clone());

        // Create a Secret
        let secret_store = ResourceStore::<Secret>::new(storage.clone());
        let secret = Secret::namespaced("my-secret", "default")
            .with_data("API_KEY", "supersecret123");
        secret_store.create(secret).await.unwrap();

        // Create a pod with env referencing the Secret
        let spec = PodSpec {
            containers: vec![Container {
                name: "test".to_string(),
                image: "nginx".to_string(),
                env: vec![EnvVar {
                    name: "API_KEY".to_string(),
                    value_from: Some(EnvVarSource {
                        secret_key_ref: Some(SecretKeySelector {
                            name: "my-secret".to_string(),
                            key: "API_KEY".to_string(),
                            optional: None,
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let pod = Pod::namespaced("test-pod", "default", spec);

        let container = &pod.spec.as_ref().unwrap().containers[0];
        let resolved = resolver.resolve_container_env(&pod, container).await.unwrap();

        assert!(resolved.env_vars.contains(&"API_KEY=supersecret123".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_configmap_volume() {
        let storage = Arc::new(SledBackend::in_memory().unwrap());
        let resolver = SecretResolver::new(storage.clone());

        // Create a ConfigMap
        let cm_store = ResourceStore::<ConfigMap>::new(storage.clone());
        let mut cm = ConfigMap::namespaced("nginx-config", "default");
        cm.data.insert("nginx.conf".to_string(), "server { listen 80; }".to_string());
        cm_store.create(cm).await.unwrap();

        // Create a pod with a ConfigMap volume
        let spec = PodSpec {
            volumes: vec![k1s_types::Volume {
                name: "config-volume".to_string(),
                source: k1s_types::VolumeSource {
                    config_map: Some(k1s_types::ConfigMapVolumeSource {
                        name: "nginx-config".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            }],
            containers: vec![Container {
                name: "nginx".to_string(),
                image: "nginx".to_string(),
                volume_mounts: vec![k1s_types::VolumeMount {
                    name: "config-volume".to_string(),
                    mount_path: "/etc/nginx".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let pod = Pod::namespaced("test-pod", "default", spec);

        let container = &pod.spec.as_ref().unwrap().containers[0];
        let resolved = resolver.resolve_container_env(&pod, container).await.unwrap();

        assert!(resolved.volume_data.contains_key("/etc/nginx"));
        let volume = resolved.volume_data.get("/etc/nginx").unwrap();
        assert_eq!(
            String::from_utf8_lossy(volume.get("nginx.conf").unwrap()),
            "server { listen 80; }"
        );
    }

    #[tokio::test]
    async fn test_optional_configmap_missing() {
        let storage = Arc::new(SledBackend::in_memory().unwrap());
        let resolver = SecretResolver::new(storage.clone());

        // Create a pod referencing a non-existent ConfigMap (optional)
        let spec = PodSpec {
            containers: vec![Container {
                name: "test".to_string(),
                image: "nginx".to_string(),
                env_from: vec![EnvFromSource {
                    config_map_ref: Some(ConfigMapEnvSource {
                        name: "missing-config".to_string(),
                        optional: Some(true),
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let pod = Pod::namespaced("test-pod", "default", spec);

        let container = &pod.spec.as_ref().unwrap().containers[0];
        let resolved = resolver.resolve_container_env(&pod, container).await;

        // Should succeed because ConfigMap is optional
        assert!(resolved.is_ok());
    }

    #[tokio::test]
    async fn test_required_configmap_missing() {
        let storage = Arc::new(SledBackend::in_memory().unwrap());
        let resolver = SecretResolver::new(storage.clone());

        // Create a pod referencing a non-existent ConfigMap (required)
        let spec = PodSpec {
            containers: vec![Container {
                name: "test".to_string(),
                image: "nginx".to_string(),
                env_from: vec![EnvFromSource {
                    config_map_ref: Some(ConfigMapEnvSource {
                        name: "missing-config".to_string(),
                        optional: Some(false),
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let pod = Pod::namespaced("test-pod", "default", spec);

        let container = &pod.spec.as_ref().unwrap().containers[0];
        let resolved = resolver.resolve_container_env(&pod, container).await;

        // Should fail because ConfigMap is required but missing
        assert!(resolved.is_err());
    }
}
