//! Health check handlers

use axum::http::StatusCode;
use axum::response::IntoResponse;

/// Liveness probe
pub async fn livez() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness probe
pub async fn readyz() -> impl IntoResponse {
    StatusCode::OK
}

/// Legacy health endpoint
pub async fn healthz() -> impl IntoResponse {
    "ok"
}
