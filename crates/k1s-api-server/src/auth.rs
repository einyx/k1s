//! Authentication middleware for the API server
//!
//! Supports:
//! - Bearer token authentication (ServiceAccount tokens)
//! - Anonymous access for health endpoints

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

/// User information extracted from authentication
#[derive(Debug, Clone)]
pub struct UserInfo {
    /// Username (e.g., "system:serviceaccount:default:default")
    pub username: String,
    /// UID of the user
    pub uid: String,
    /// Groups the user belongs to
    pub groups: Vec<String>,
    /// Extra information
    pub extra: std::collections::HashMap<String, Vec<String>>,
}

impl Default for UserInfo {
    fn default() -> Self {
        Self {
            username: "system:anonymous".to_string(),
            uid: "".to_string(),
            groups: vec!["system:unauthenticated".to_string()],
            extra: std::collections::HashMap::new(),
        }
    }
}

impl UserInfo {
    /// Create a system admin user (for internal operations)
    pub fn system_admin() -> Self {
        Self {
            username: "system:admin".to_string(),
            uid: "system:admin".to_string(),
            groups: vec![
                "system:masters".to_string(),
                "system:authenticated".to_string(),
            ],
            extra: std::collections::HashMap::new(),
        }
    }

    /// Create a ServiceAccount user
    pub fn service_account(namespace: &str, name: &str, uid: &str) -> Self {
        Self {
            username: format!("system:serviceaccount:{}:{}", namespace, name),
            uid: uid.to_string(),
            groups: vec![
                format!("system:serviceaccounts:{}", namespace),
                "system:serviceaccounts".to_string(),
                "system:authenticated".to_string(),
            ],
            extra: std::collections::HashMap::new(),
        }
    }

    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.groups.contains(&"system:authenticated".to_string())
    }

    /// Check if user is in system:masters group
    pub fn is_admin(&self) -> bool {
        self.groups.contains(&"system:masters".to_string())
    }
}

/// Token claims for ServiceAccount tokens
#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceAccountToken {
    pub namespace: String,
    pub name: String,
    pub uid: String,
}

/// Authentication error response
#[derive(Debug, Serialize)]
struct AuthError {
    kind: String,
    #[serde(rename = "apiVersion")]
    api_version: String,
    status: String,
    message: String,
    reason: String,
    code: u16,
}

impl AuthError {
    fn unauthorized(message: &str) -> Self {
        Self {
            kind: "Status".to_string(),
            api_version: "v1".to_string(),
            status: "Failure".to_string(),
            message: message.to_string(),
            reason: "Unauthorized".to_string(),
            code: 401,
        }
    }
}

/// Paths that don't require authentication
const ANONYMOUS_PATHS: &[&str] = &[
    "/healthz",
    "/livez",
    "/readyz",
    "/openapi/v2",
    "/openapi/v3",
    "/version",
];

/// Authentication middleware
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Allow anonymous access to health endpoints
    if ANONYMOUS_PATHS.iter().any(|p| path.starts_with(p)) {
        request.extensions_mut().insert(UserInfo::default());
        return next.run(request).await;
    }

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let user_info = match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            match validate_token(&state, token).await {
                Ok(user) => user,
                Err(msg) => {
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(AuthError::unauthorized(&msg)),
                    )
                        .into_response();
                }
            }
        }
        Some(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuthError::unauthorized("Invalid authorization header format")),
            )
                .into_response();
        }
        None => {
            // Allow anonymous access but mark as unauthenticated
            // RBAC will decide if the request is allowed
            UserInfo::default()
        }
    };

    // Store user info in request extensions for use by handlers
    request.extensions_mut().insert(user_info);

    next.run(request).await
}

/// Validate a bearer token and return user info
async fn validate_token(state: &AppState, token: &str) -> Result<UserInfo, String> {
    // Check for static admin token (for bootstrapping)
    // In production, this should be a secure randomly generated token
    if token == "k1s-admin-token" {
        return Ok(UserInfo::system_admin());
    }

    // Try to parse as a ServiceAccount token
    // Format: base64(namespace:name:uid)
    if let Ok(decoded) = base64_decode(token) {
        let parts: Vec<&str> = decoded.split(':').collect();
        if parts.len() == 3 {
            let (namespace, name, uid) = (parts[0], parts[1], parts[2]);

            // Verify the ServiceAccount exists
            use k1s_storage::backend::ResourceStore;
            use k1s_types::ServiceAccount;

            let sa_store = ResourceStore::<ServiceAccount>::new(state.storage.clone());
            match sa_store.get(Some(namespace), name).await {
                Ok(Some(sa)) if sa.metadata.uid == uid => {
                    return Ok(UserInfo::service_account(namespace, name, uid));
                }
                Ok(Some(_)) => {
                    return Err("Token UID mismatch".to_string());
                }
                Ok(None) => {
                    return Err(format!(
                        "ServiceAccount {}/{} not found",
                        namespace, name
                    ));
                }
                Err(e) => {
                    return Err(format!("Failed to verify ServiceAccount: {}", e));
                }
            }
        }
    }

    Err("Invalid token format".to_string())
}

/// Simple base64 decode helper
fn base64_decode(input: &str) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD
        .decode(input)
        .map_err(|e| e.to_string())
        .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
}

/// Generate a token for a ServiceAccount
pub fn generate_sa_token(namespace: &str, name: &str, uid: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.encode(format!("{}:{}:{}", namespace, name, uid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_info_default() {
        let user = UserInfo::default();
        assert_eq!(user.username, "system:anonymous");
        assert!(!user.is_authenticated());
        assert!(!user.is_admin());
    }

    #[test]
    fn test_user_info_admin() {
        let user = UserInfo::system_admin();
        assert!(user.is_authenticated());
        assert!(user.is_admin());
    }

    #[test]
    fn test_service_account_user() {
        let user = UserInfo::service_account("default", "my-sa", "uid-123");
        assert_eq!(user.username, "system:serviceaccount:default:my-sa");
        assert!(user.is_authenticated());
        assert!(!user.is_admin());
        assert!(user.groups.contains(&"system:serviceaccounts:default".to_string()));
    }

    #[test]
    fn test_token_generation() {
        let token = generate_sa_token("default", "test", "uid-123");
        let decoded = base64_decode(&token).unwrap();
        assert_eq!(decoded, "default:test:uid-123");
    }
}
