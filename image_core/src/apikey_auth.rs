//! Bearer-token authentication for public image generation endpoints
//! (Bearer sk-... keys stored in `api_keys`).

use crate::models::ApiKey;
use crate::state::AppState;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, Clone)]
pub struct AuthedKey {
    pub id: String,
    pub name: String,
}

/// Middleware that requires an enabled API key in `Authorization: Bearer <key>`.
/// On failure returns an OpenAI-shaped 401.
pub async fn require_api_key(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let header_val = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let token = match header_val.as_deref() {
        Some(s) if s.starts_with("Bearer ") => s[7..].trim().to_string(),
        _ => return unauthorized("missing_credentials", "Missing or malformed Authorization header."),
    };

    match lookup_key(&state, &token).await {
        Some(key) => {
            req.extensions_mut().insert(AuthedKey {
                id: key.id,
                name: key.name,
            });
            next.run(req).await
        }
        None => unauthorized("invalid_api_key", "Invalid or disabled API key."),
    }
}

async fn lookup_key(state: &AppState, token: &str) -> Option<ApiKey> {
    sqlx::query_as::<_, ApiKeyRow>(
        "SELECT id, name, key, enabled, created_at FROM api_keys WHERE key = ?",
    )
    .bind(token)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .map(|r| ApiKey {
        id: r.id,
        name: r.name,
        key: r.key,
        enabled: r.enabled != 0,
        created_at: r.created_at,
    })
    .filter(|k| k.enabled)
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: String,
    name: String,
    key: String,
    enabled: i64,
    created_at: i64,
}

fn unauthorized(code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message,
            "type": "invalid_request_error"
        }
    });
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}