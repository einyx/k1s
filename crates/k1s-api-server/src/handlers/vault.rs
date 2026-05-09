//! Vault handlers for secret management

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use k1s_vault::{AuthInfo, VaultError};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// ============================================================================
// Transit Engine Handlers
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    pub key_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateKeyResponse {
    pub created: bool,
}

pub async fn transit_create_key(
    State(state): State<AppState>,
    Path(key_name): Path<String>,
) -> ApiResult<Json<CreateKeyResponse>> {
    state.vault.transit.create_key(&key_name).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(CreateKeyResponse { created: true }))
}

#[derive(Debug, Deserialize)]
pub struct EncryptRequest {
    pub plaintext: String,
}

#[derive(Debug, Serialize)]
pub struct EncryptResponse {
    pub ciphertext: String,
}

pub async fn transit_encrypt(
    State(state): State<AppState>,
    Path(key_name): Path<String>,
    Json(request): Json<EncryptRequest>,
) -> ApiResult<Json<EncryptResponse>> {
    let auth = AuthInfo {
        user: "admin".to_string(), // TODO: extract from auth context
        namespace: None,
        source_ip: None,
    };

    let plaintext = BASE64.decode(&request.plaintext)
        .map_err(|e| ApiError::BadRequest(format!("Invalid base64: {}", e)))?;

    let ciphertext = state.vault.transit.encrypt(&key_name, &plaintext, auth).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(EncryptResponse { ciphertext }))
}

#[derive(Debug, Deserialize)]
pub struct DecryptRequest {
    pub ciphertext: String,
}

#[derive(Debug, Serialize)]
pub struct DecryptResponse {
    pub plaintext: String,
}

pub async fn transit_decrypt(
    State(state): State<AppState>,
    Path(key_name): Path<String>,
    Json(request): Json<DecryptRequest>,
) -> ApiResult<Json<DecryptResponse>> {
    let auth = AuthInfo {
        user: "admin".to_string(), // TODO: extract from auth context
        namespace: None,
        source_ip: None,
    };

    let plaintext = state.vault.transit.decrypt(&key_name, &request.ciphertext, auth).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let plaintext_b64 = BASE64.encode(&plaintext);

    Ok(Json(DecryptResponse { plaintext: plaintext_b64 }))
}

#[derive(Debug, Serialize)]
pub struct ListKeysResponse {
    pub keys: Vec<String>,
}

pub async fn transit_list_keys(
    State(state): State<AppState>,
) -> ApiResult<Json<ListKeysResponse>> {
    let keys = state.vault.transit.list_keys().await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(ListKeysResponse { keys }))
}

pub async fn transit_delete_key(
    State(state): State<AppState>,
    Path(key_name): Path<String>,
) -> ApiResult<StatusCode> {
    state.vault.transit.delete_key(&key_name).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// KV Engine Handlers
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct KvReadQuery {
    pub version: Option<u32>,
}

pub async fn kv_read(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<KvReadQuery>,
) -> ApiResult<impl IntoResponse> {
    let auth = AuthInfo {
        user: "admin".to_string(),
        namespace: None,
        source_ip: None,
    };

    let secret = state.vault.kv.read(&path, query.version, auth).await
        .map_err(|e| match e {
            VaultError::SecretNotFound(_) => ApiError::NotFound {
                kind: "Secret".to_string(),
                name: path.clone(),
            },
            _ => ApiError::Internal(e.to_string()),
        })?;

    Ok(Json(serde_json::to_value(secret).unwrap()))
}

#[derive(Debug, Deserialize)]
pub struct KvWriteRequest {
    pub data: HashMap<String, String>,
    pub cas: Option<u32>,
}

pub async fn kv_write(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Json(request): Json<KvWriteRequest>,
) -> ApiResult<impl IntoResponse> {
    let auth = AuthInfo {
        user: "admin".to_string(),
        namespace: None,
        source_ip: None,
    };

    let metadata = state.vault.kv.write(&path, request.data, request.cas, auth).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::to_value(metadata).unwrap()))
}

#[derive(Debug, Deserialize)]
pub struct KvDeleteRequest {
    pub versions: Option<Vec<u32>>,
}

pub async fn kv_delete(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Json(request): Json<KvDeleteRequest>,
) -> ApiResult<StatusCode> {
    let auth = AuthInfo {
        user: "admin".to_string(),
        namespace: None,
        source_ip: None,
    };

    state.vault.kv.delete(&path, request.versions, auth).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn kv_list(
    State(state): State<AppState>,
    Path(prefix): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let secrets = state.vault.kv.list(&prefix).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "keys": secrets })))
}

// ============================================================================
// PKI Engine Handlers
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GenerateRootRequest {
    pub common_name: String,
    pub ttl_days: Option<i64>,
}

#[axum::debug_handler]
pub async fn pki_generate_root(
    State(state): State<AppState>,
    Path(ca_name): Path<String>,
    Json(request): Json<GenerateRootRequest>,
) -> ApiResult<impl IntoResponse> {
    let ttl_days = request.ttl_days.unwrap_or(365);

    let ca = state.vault.pki.generate_root(&ca_name, &request.common_name, ttl_days).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::to_value(ca).unwrap()))
}

#[derive(Debug, Deserialize)]
pub struct IssueCertificateRequest {
    pub common_name: String,
    pub alt_names: Option<Vec<String>>,
    pub ip_sans: Option<Vec<String>>,
    pub ttl: Option<String>,
}

pub async fn pki_issue_certificate(
    State(state): State<AppState>,
    Path(ca_name): Path<String>,
    Json(request): Json<IssueCertificateRequest>,
) -> ApiResult<impl IntoResponse> {
    let auth = AuthInfo {
        user: "admin".to_string(),
        namespace: None,
        source_ip: None,
    };

    let issue_request = k1s_vault::engines::IssueRequest {
        common_name: request.common_name,
        alt_names: request.alt_names.unwrap_or_default(),
        ip_sans: request.ip_sans.unwrap_or_default(),
        ttl: request.ttl.unwrap_or_else(|| "24h".to_string()),
    };

    let cert = state.vault.pki.issue_certificate(&ca_name, issue_request, auth).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::to_value(cert).unwrap()))
}

#[derive(Debug, Deserialize)]
pub struct RevokeCertificateRequest {
    pub serial_number: String,
}

pub async fn pki_revoke_certificate(
    State(state): State<AppState>,
    Path(ca_name): Path<String>,
    Json(request): Json<RevokeCertificateRequest>,
) -> ApiResult<StatusCode> {
    let auth = AuthInfo {
        user: "admin".to_string(),
        namespace: None,
        source_ip: None,
    };

    state.vault.pki.revoke_certificate(&ca_name, &request.serial_number, auth).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn pki_list_certificates(
    State(state): State<AppState>,
    Path(ca_name): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let serials = state.vault.pki.list_certificates(&ca_name).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "certificates": serials })))
}

// ============================================================================
// Audit Engine Handlers
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AuditQueryRequest {
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn audit_query(
    State(state): State<AppState>,
    Query(query): Query<AuditQueryRequest>,
) -> ApiResult<impl IntoResponse> {
    let entries = state.vault.audit.query(query.start_time, query.end_time).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "entries": entries })))
}
