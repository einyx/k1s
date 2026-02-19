//! TLS certificate generation and management

use std::path::Path;

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType, IsCa,
    KeyPair, KeyUsagePurpose, SanType,
};
use tracing::info;

#[derive(Debug, Clone)]
pub struct TlsCerts {
    pub ca_cert_pem: String,
    pub server_cert_pem: String,
    pub server_key_pem: String,
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
        let ca_key = KeyPair::generate()?;
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
        let ca_cert = ca_params.self_signed(&ca_key)?;

        let server_key = KeyPair::generate()?;
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
            san.push(SanType::DnsName(dns.clone().try_into()?));
        }
        for ip in &config.san_ips {
            san.push(SanType::IpAddress(*ip));
        }
        server_params.subject_alt_names = san;

        let mut server_dn = DistinguishedName::new();
        server_dn.push(DnType::CommonName, "k1s-server");
        server_dn.push(DnType::OrganizationName, "k1s");
        server_params.distinguished_name = server_dn;

        let server_cert = server_params.signed_by(&server_key, &ca_cert, &ca_key)?;

        Ok(Self {
            ca_cert_pem: ca_cert.pem(),
            server_cert_pem: server_cert.pem(),
            server_key_pem: server_key.serialize_pem(),
        })
    }

    pub fn load_or_generate(config: &TlsConfig) -> anyhow::Result<Self> {
        let ca_path = config.cert_dir.join("ca.crt");
        let cert_path = config.cert_dir.join("server.crt");
        let key_path = config.cert_dir.join("server.key");

        if ca_path.exists() && cert_path.exists() && key_path.exists() {
            info!("Loading existing TLS certificates from {:?}", config.cert_dir);
            let ca_cert_pem = std::fs::read_to_string(&ca_path)?;
            let server_cert_pem = std::fs::read_to_string(&cert_path)?;
            let server_key_pem = std::fs::read_to_string(&key_path)?;
            return Ok(Self {
                ca_cert_pem,
                server_cert_pem,
                server_key_pem,
            });
        }

        info!("Generating new TLS certificates in {:?}", config.cert_dir);
        std::fs::create_dir_all(&config.cert_dir)?;
        let certs = Self::generate(config)?;

        std::fs::write(&ca_path, &certs.ca_cert_pem)?;
        std::fs::write(&cert_path, &certs.server_cert_pem)?;
        std::fs::write(&key_path, &certs.server_key_pem)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(certs)
    }

    pub fn generate_kubeconfig(&self, server_url: &str) -> String {
        use base64::{Engine, engine::general_purpose::STANDARD};

        let ca_data = STANDARD.encode(&self.ca_cert_pem);

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
    client-certificate-data: {ca_data}
    client-key-data: {ca_data}
"#,
            ca_data = ca_data,
            server_url = server_url,
        )
    }
}
