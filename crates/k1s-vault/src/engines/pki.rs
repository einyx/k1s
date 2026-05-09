//! PKI engine: Certificate Authority and certificate management
//!
//! Provides CA setup, certificate issuance, and revocation

use chrono::{DateTime, Duration, Utc};
use rsa::{RsaPrivateKey, RsaPublicKey};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use k1s_storage::{SledBackend, Storage};
use crate::audit::{AuditLogger, AuditOperation, AuthInfo};
use crate::error::{VaultError, VaultResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateAuthority {
    pub name: String,
    pub certificate: String,
    pub private_key: String,
    pub serial_number: u64,
    pub created_at: DateTime<Utc>,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuedCertificate {
    pub serial_number: String,
    pub certificate: String,
    pub private_key: String,
    pub ca_chain: Vec<String>,
    pub common_name: String,
    pub issuing_ca: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}

#[derive(Debug, Clone)]
pub struct IssueRequest {
    pub common_name: String,
    pub alt_names: Vec<String>,
    pub ip_sans: Vec<String>,
    pub ttl: String,  // e.g., "24h", "30d"
}

pub struct PkiEngine {
    storage: Arc<SledBackend>,
    audit: Arc<AuditLogger>,
}

impl PkiEngine {
    pub fn new(storage: Arc<SledBackend>, audit: Arc<AuditLogger>) -> VaultResult<Self> {
        Ok(Self { storage, audit })
    }

    /// Generate a new CA
    pub async fn generate_root(
        &self,
        name: &str,
        common_name: &str,
        ttl_days: i64,
    ) -> VaultResult<CertificateAuthority> {
        // Check if CA exists
        let ca_path = format!("pki/ca/{name}");
        if self.storage.get(&ca_path).await?.is_some() {
            return Err(VaultError::InvalidInput(format!("CA {name} already exists")));
        }

        // Generate RSA keypair
        use rsa::rand_core::OsRng;
        let private_key = RsaPrivateKey::new(&mut OsRng, 2048)
            .map_err(|e| VaultError::CertificateFailed(format!("Key generation failed: {e}")))?;

        let _public_key = RsaPublicKey::from(&private_key);

        // Create self-signed certificate (simplified - real impl would use x509-cert properly)
        let not_before = Utc::now();
        let not_after = not_before + Duration::days(ttl_days);

        let private_key_pem = private_key.to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| VaultError::CertificateFailed(format!("PEM encoding failed: {e}")))?
            .to_string();

        // Simplified cert (in production, use x509-cert builder)
        let cert_pem = format!(
            "-----BEGIN CERTIFICATE-----\n\
            Simplified CA Certificate for {name}\n\
            CN: {common_name}\n\
            Not Before: {not_before}\n\
            Not After: {not_after}\n\
            -----END CERTIFICATE-----"
        );

        let ca = CertificateAuthority {
            name: name.to_string(),
            certificate: cert_pem,
            private_key: private_key_pem,
            serial_number: 1,
            created_at: Utc::now(),
            not_before,
            not_after,
        };

        // Store CA
        let data = serde_json::to_vec(&ca)
            .map_err(|e| VaultError::Internal(e.to_string()))?;
        self.storage.put(&ca_path, data).await?;

        tracing::info!("Generated root CA: {}", name);
        Ok(ca)
    }

    /// Issue a certificate
    pub async fn issue_certificate(
        &self,
        ca_name: &str,
        request: IssueRequest,
        auth: AuthInfo,
    ) -> VaultResult<IssuedCertificate> {
        // Audit log
        let entry = self.audit.request(
            AuditOperation::PkiIssue,
            auth,
            format!("/pki/issue/{ca_name}"),
            Some(serde_json::json!({
                "common_name": request.common_name,
                "ttl": request.ttl,
            })),
        );

        let result: VaultResult<IssuedCertificate> = async {
            // Load CA
            let ca_path = format!("pki/ca/{ca_name}");
            let ca_data = self.storage.get(&ca_path).await?
                .ok_or_else(|| VaultError::KeyNotFound(format!("CA {ca_name}")))?;

            let mut ca: CertificateAuthority = serde_json::from_slice(&ca_data)
                .map_err(|e| VaultError::Internal(e.to_string()))?;

            // Parse TTL
            let ttl_duration = self.parse_ttl(&request.ttl)?;
            let issued_at = Utc::now();
            let expires_at = issued_at + ttl_duration;

            // Generate keypair for certificate
            use rsa::rand_core::OsRng;
            let private_key = RsaPrivateKey::new(&mut OsRng, 2048)
                .map_err(|e| VaultError::CertificateFailed(format!("Key generation failed: {e}")))?;

            let private_key_pem = private_key.to_pkcs8_pem(LineEnding::LF)
                .map_err(|e| VaultError::CertificateFailed(format!("PEM encoding failed: {e}")))?
                .to_string();

            // Generate serial number
            let serial = format!("{:016x}", ca.serial_number);
            ca.serial_number += 1;

            // Create certificate (simplified)
            let cert_pem = format!(
                "-----BEGIN CERTIFICATE-----\n\
                Certificate Serial: {}\n\
                CN: {}\n\
                Issuer: {}\n\
                Not Before: {}\n\
                Not After: {}\n\
                Alt Names: {:?}\n\
                IP SANs: {:?}\n\
                -----END CERTIFICATE-----",
                serial,
                request.common_name,
                ca_name,
                issued_at,
                expires_at,
                request.alt_names,
                request.ip_sans
            );

            let certificate = IssuedCertificate {
                serial_number: serial.clone(),
                certificate: cert_pem,
                private_key: private_key_pem,
                ca_chain: vec![ca.certificate.clone()],
                common_name: request.common_name.clone(),
                issuing_ca: ca_name.to_string(),
                issued_at,
                expires_at,
                revoked: false,
            };

            // Store certificate
            let cert_path = format!("pki/certs/{ca_name}/{serial}");
            let cert_data = serde_json::to_vec(&certificate)
                .map_err(|e| VaultError::Internal(e.to_string()))?;
            self.storage.put(&cert_path, cert_data).await?;

            // Update CA serial number
            let ca_data = serde_json::to_vec(&ca)
                .map_err(|e| VaultError::Internal(e.to_string()))?;
            self.storage.put(&ca_path, ca_data).await?;

            Ok(certificate)
        }.await;

        // Audit response
        match &result {
            Ok(_) => {
                let entry = self.audit.response(entry, "success".to_string(), None);
                self.audit.log(entry).await;
            }
            Err(e) => {
                let entry = self.audit.response(entry, "error".to_string(), Some(e.to_string()));
                self.audit.log(entry).await;
            }
        }

        result
    }

    /// Revoke a certificate
    pub async fn revoke_certificate(
        &self,
        ca_name: &str,
        serial: &str,
        auth: AuthInfo,
    ) -> VaultResult<()> {
        // Audit log
        let entry = self.audit.request(
            AuditOperation::PkiRevoke,
            auth,
            format!("/pki/revoke/{ca_name}/{serial}"),
            None,
        );

        let result: VaultResult<()> = async {
            let cert_path = format!("pki/certs/{ca_name}/{serial}");
            let cert_data = self.storage.get(&cert_path).await?
                .ok_or_else(|| VaultError::SecretNotFound(format!("Certificate {serial}")))?;

            let mut certificate: IssuedCertificate = serde_json::from_slice(&cert_data)
                .map_err(|e| VaultError::Internal(e.to_string()))?;

            certificate.revoked = true;

            let updated_data = serde_json::to_vec(&certificate)
                .map_err(|e| VaultError::Internal(e.to_string()))?;
            self.storage.put(&cert_path, updated_data).await?;

            tracing::info!("Revoked certificate: {}", serial);
            Ok(())
        }.await;

        // Audit response
        match &result {
            Ok(_) => {
                let entry = self.audit.response(entry, "success".to_string(), None);
                self.audit.log(entry).await;
            }
            Err(e) => {
                let entry = self.audit.response(entry, "error".to_string(), Some(e.to_string()));
                self.audit.log(entry).await;
            }
        }

        result
    }

    /// List certificates for a CA
    pub async fn list_certificates(&self, ca_name: &str) -> VaultResult<Vec<String>> {
        let prefix = format!("pki/certs/{ca_name}/");
        let entries = self.storage.list(&prefix).await?;

        let serials: Vec<String> = entries
            .into_iter()
            .filter_map(|(path, _)| {
                path.strip_prefix(&prefix).map(|s| s.to_string())
            })
            .collect();

        Ok(serials)
    }

    /// Parse TTL string (e.g., "24h", "30d", "1y")
    fn parse_ttl(&self, ttl: &str) -> VaultResult<Duration> {
        let (num_str, unit) = ttl.split_at(ttl.len() - 1);
        let num: i64 = num_str.parse()
            .map_err(|_| VaultError::InvalidInput(format!("Invalid TTL: {ttl}")))?;

        match unit {
            "h" => Ok(Duration::hours(num)),
            "d" => Ok(Duration::days(num)),
            "w" => Ok(Duration::weeks(num)),
            "y" => Ok(Duration::days(num * 365)),
            _ => Err(VaultError::InvalidInput(format!("Invalid TTL unit: {unit}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pki_issue_cert() {
        let storage = Arc::new(SledBackend::in_memory().unwrap());
        let audit = Arc::new(AuditLogger::new(storage.clone()));
        let pki = PkiEngine::new(storage, audit).unwrap();

        // Generate CA
        pki.generate_root("test-ca", "Test CA", 365).await.unwrap();

        // Issue certificate
        let auth = AuthInfo {
            user: "test".to_string(),
            namespace: None,
            source_ip: None,
        };

        let request = IssueRequest {
            common_name: "test.example.com".to_string(),
            alt_names: vec!["test2.example.com".to_string()],
            ip_sans: vec!["192.168.1.1".to_string()],
            ttl: "24h".to_string(),
        };

        let cert = pki.issue_certificate("test-ca", request, auth).await.unwrap();
        assert_eq!(cert.common_name, "test.example.com");
        assert!(!cert.revoked);
    }
}
