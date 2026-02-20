//! API server implementation

use std::net::SocketAddr;
use std::sync::Arc;

use axum::middleware;
use axum_server::tls_rustls::RustlsConfig;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;

use k1s_storage::SledBackend;

use crate::auth::auth_middleware;
use crate::authz::{authz_middleware, create_default_rbac};
use crate::routes::build_router;
use crate::state::AppState;
use crate::tls::{TlsCerts, TlsConfig};
use crate::webhooks::{WebhookState, admission_webhook_middleware};

/// API server configuration
#[derive(Debug, Clone)]
pub struct ApiServerConfig {
    pub bind_address: SocketAddr,
    pub data_dir: std::path::PathBuf,
    pub node_name: String,
    pub tls_enabled: bool,
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:6443".parse().unwrap(),
            data_dir: std::path::PathBuf::from("/var/lib/k1s"),
            node_name: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "k1s-node".to_string()),
            tls_enabled: true,
        }
    }
}

/// The k1s API server
pub struct ApiServer {
    config: ApiServerConfig,
    storage: Arc<SledBackend>,
    tls_certs: Option<TlsCerts>,
}

impl ApiServer {
    /// Create a new API server with the given configuration
    pub fn new(config: ApiServerConfig) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;

        let storage_path = config.data_dir.join("storage");
        let storage = Arc::new(SledBackend::open(&storage_path)?);

        let tls_certs = if config.tls_enabled {
            let tls_config = TlsConfig::new(config.data_dir.join("pki"));
            Some(TlsCerts::load_or_generate(&tls_config)?)
        } else {
            None
        };

        Ok(Self { config, storage, tls_certs })
    }

    /// Create an API server with in-memory storage (for testing)
    pub fn in_memory(config: ApiServerConfig) -> anyhow::Result<Self> {
        let storage = Arc::new(SledBackend::in_memory()?);
        Ok(Self { config, storage, tls_certs: None })
    }

    /// Get a reference to the storage backend
    pub fn storage(&self) -> Arc<SledBackend> {
        self.storage.clone()
    }

    /// Get TLS certificates if enabled
    pub fn tls_certs(&self) -> Option<&TlsCerts> {
        self.tls_certs.as_ref()
    }

    /// Generate kubeconfig for this server
    pub fn kubeconfig(&self) -> Option<String> {
        let scheme = if self.tls_certs.is_some() { "https" } else { "http" };
        let server_url = format!("{}://127.0.0.1:{}", scheme, self.config.bind_address.port());
        self.tls_certs.as_ref().map(|certs| certs.generate_kubeconfig(&server_url))
    }

    /// Run the API server
    pub async fn run(self) -> anyhow::Result<()> {
        let state = AppState::new(self.storage.clone(), self.config.node_name.clone());

        // Create default RBAC resources
        if let Err(e) = create_default_rbac(&state).await {
            tracing::warn!("Failed to create default RBAC resources: {}", e);
        }

        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        // Create webhook state
        let webhook_state = WebhookState::new(self.storage.clone());

        // Build router with authentication, authorization, and admission webhook middleware
        // Middleware is applied in reverse order (last added runs first)
        // Order: TraceLayer -> CORS -> Auth -> Authz -> Webhooks -> Handlers
        let app = build_router(state.clone())
            .layer(middleware::from_fn_with_state(webhook_state, admission_webhook_middleware))
            .layer(middleware::from_fn_with_state(state.clone(), authz_middleware))
            .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
            .layer(cors)
            .layer(TraceLayer::new_for_http());

        info!("Starting API server on {}", self.config.bind_address);
        info!("Authentication, RBAC authorization, and admission webhooks enabled");

        if let Some(ref certs) = self.tls_certs {
            info!("TLS enabled");
            let rustls_config = RustlsConfig::from_pem(
                certs.server_cert_pem.as_bytes().to_vec(),
                certs.server_key_pem.as_bytes().to_vec(),
            ).await?;

            axum_server::bind_rustls(self.config.bind_address, rustls_config)
                .serve(app.into_make_service())
                .await?;
        } else {
            info!("TLS disabled - running in insecure mode");
            let listener = tokio::net::TcpListener::bind(self.config.bind_address).await?;
            axum::serve(listener, app).await?;
        }

        Ok(())
    }

    /// Initialize default resources
    pub async fn init_defaults(&self) -> anyhow::Result<()> {
        use k1s_storage::backend::ResourceStore;
        use k1s_types::{Namespace, NamespacePhase, NamespaceStatus};

        let ns_store = ResourceStore::<Namespace>::new(self.storage.clone());

        // Create default namespace if it doesn't exist
        if ns_store.get(None, "default").await?.is_none() {
            let mut default_ns = Namespace::new("default");
            default_ns.status = Some(NamespaceStatus {
                phase: Some(NamespacePhase::Active),
                conditions: vec![],
            });
            ns_store.create(default_ns).await?;
            info!("Created default namespace");
        }

        // Create kube-system namespace if it doesn't exist
        if ns_store.get(None, "kube-system").await?.is_none() {
            let mut kube_system = Namespace::new("kube-system");
            kube_system.status = Some(NamespaceStatus {
                phase: Some(NamespacePhase::Active),
                conditions: vec![],
            });
            ns_store.create(kube_system).await?;
            info!("Created kube-system namespace");
        }

        // Create kube-public namespace if it doesn't exist
        if ns_store.get(None, "kube-public").await?.is_none() {
            let mut kube_public = Namespace::new("kube-public");
            kube_public.status = Some(NamespaceStatus {
                phase: Some(NamespacePhase::Active),
                conditions: vec![],
            });
            ns_store.create(kube_public).await?;
            info!("Created kube-public namespace");
        }

        Ok(())
    }
}
