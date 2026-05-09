//! TLS certificate generation and management

use std::path::Path;

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType, IsCa,
    KeyUsagePurpose, SanType,
};
use tracing::info;

#[derive(Debug, Clone)]
pub struct TlsCerts {
    pub ca_cert_pem: String,
    pub server_cert_pem: String,
    pub server_key_pem: String,
    pub client_cert_pem: String,
    pub client_key_pem: String,
}

pub struct TlsConfig {
    pub cert_dir: std::path::PathBuf,
    pub san_dns: Vec<String>,
    pub san_ips: Vec<std::net::IpAddr>,
}

impl TlsConfig {
    pub fn new(cert_dir: impl AsRef<Path>) -> Self {
        Self {
            cert_dir: cert_dir.as_ref().to_path_buf(),
            san_dns: vec![
                "localhost".to_string(),
                "kubernetes".to_string(),
                "kubernetes.default".to_string(),
                "kubernetes.default.svc".to_string(),
            ],
            san_ips: vec![
                "127.0.0.1".parse().unwrap(),
                "::1".parse().unwrap(),
            ],
        }
    }
}

impl TlsCerts {
    pub fn generate(config: &TlsConfig) -> anyhow::Result<Self> {
        // Generate CA certificate
        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CommonName, "k1s-ca");
        ca_dn.push(DnType::OrganizationName, "k1s");
        ca_params.distinguished_name = ca_dn;

        let ca_cert = Certificate::from_params(ca_params)?;
        let ca_cert_pem = ca_cert.serialize_pem()?;
        let ca_key_pem = ca_cert.serialize_private_key_pem();

        // Generate server certificate signed by CA
        let mut server_params = CertificateParams::default();
        server_params.is_ca = IsCa::NoCa;
        server_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        server_params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ServerAuth,
        ];

        let mut san = Vec::new();
        for dns in &config.san_dns {
            san.push(SanType::DnsName(dns.clone()));
        }
        for ip in &config.san_ips {
            san.push(SanType::IpAddress(*ip));
        }
        server_params.subject_alt_names = san;

        let mut server_dn = DistinguishedName::new();
        server_dn.push(DnType::CommonName, "k1s-server");
        server_dn.push(DnType::OrganizationName, "k1s");
        server_params.distinguished_name = server_dn;

        let server_cert = Certificate::from_params(server_params)?;
        let server_cert_pem = server_cert.serialize_pem_with_signer(&ca_cert)?;
        let server_key_pem = server_cert.serialize_private_key_pem();

        // Generate client certificate signed by CA (for admin kubeconfig)
        let mut client_params = CertificateParams::default();
        client_params.is_ca = IsCa::NoCa;
        client_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        client_params.extended_key_usages = vec![
            rcgen::ExtendedKeyUsagePurpose::ClientAuth,
        ];

        let mut client_dn = DistinguishedName::new();
        client_dn.push(DnType::CommonName, "k1s-admin");
        client_dn.push(DnType::OrganizationName, "system:masters");
        client_params.distinguished_name = client_dn;

        let client_cert = Certificate::from_params(client_params)?;
        let client_cert_pem = client_cert.serialize_pem_with_signer(&ca_cert)?;
        let client_key_pem = client_cert.serialize_private_key_pem();

        Ok(Self {
            ca_cert_pem,
            server_cert_pem,
            server_key_pem,
            client_cert_pem,
            client_key_pem,
        })
    }

    pub fn load_or_generate(config: &TlsConfig) -> anyhow::Result<Self> {
        let ca_path = config.cert_dir.join("ca.crt");
        let cert_path = config.cert_dir.join("server.crt");
        let key_path = config.cert_dir.join("server.key");
        let client_cert_path = config.cert_dir.join("client.crt");
        let client_key_path = config.cert_dir.join("client.key");

        if ca_path.exists() && cert_path.exists() && key_path.exists()
            && client_cert_path.exists() && client_key_path.exists()
        {
            info!("Loading existing TLS certificates from {:?}", config.cert_dir);
            let ca_cert_pem = std::fs::read_to_string(&ca_path)?;
            let server_cert_pem = std::fs::read_to_string(&cert_path)?;
            let server_key_pem = std::fs::read_to_string(&key_path)?;
            let client_cert_pem = std::fs::read_to_string(&client_cert_path)?;
            let client_key_pem = std::fs::read_to_string(&client_key_path)?;
            return Ok(Self {
                ca_cert_pem,
                server_cert_pem,
                server_key_pem,
                client_cert_pem,
                client_key_pem,
            });
        }

        info!("Generating new TLS certificates in {:?}", config.cert_dir);
        std::fs::create_dir_all(&config.cert_dir)?;
        let certs = Self::generate(config)?;

        std::fs::write(&ca_path, &certs.ca_cert_pem)?;
        std::fs::write(&cert_path, &certs.server_cert_pem)?;
        std::fs::write(&key_path, &certs.server_key_pem)?;
        std::fs::write(&client_cert_path, &certs.client_cert_pem)?;
        std::fs::write(&client_key_path, &certs.client_key_pem)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
            std::fs::set_permissions(&client_key_path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(certs)
    }

    pub fn generate_kubeconfig(&self, server_url: &str) -> String {
        use base64::{Engine, engine::general_purpose::STANDARD};

        let ca_data = STANDARD.encode(&self.ca_cert_pem);
        let client_cert_data = STANDARD.encode(&self.client_cert_pem);
        let client_key_data = STANDARD.encode(&self.client_key_pem);

        format!(
            r#"apiVersion: v1
kind: Config
clusters:
- cluster:
    certificate-authority-data: {ca_data}
    server: {server_url}
  name: k1s
contexts:
- context:
    cluster: k1s
    user: k1s-admin
  name: k1s
current-context: k1s
users:
- name: k1s-admin
  user:
    client-certificate-data: {client_cert_data}
    client-key-data: {client_key_data}
"#,
        )
    }
}
