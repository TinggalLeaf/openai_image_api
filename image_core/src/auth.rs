//! Admin password authentication.
//!
//! When `ADMIN_PASSWORD` is set (non-empty), every `/admin/api/*` request
//! except `/admin/api/login` and `/admin/api/auth/status` must carry a
//! `Bearer` token previously issued by `POST /admin/api/login`.
//!
//! Tokens are stored in memory only — they survive no restarts. There is no
//! expiry.

use crate::state::AppState;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rand::RngCore;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Serialize)]
pub struct AuthStatus {
    pub auth_required: bool,
}

/// Middleware: require `Authorization: Bearer <token>` for every admin
/// endpoint. Pass through when admin auth is disabled (no ADMIN_PASSWORD).
pub async fn require_admin_token(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !state.cfg.admin_auth_enabled() {
        return next.run(req).await;
    }
    let header_val = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let token = match header_val.as_deref() {
        Some(s) if s.starts_with("Bearer ") => s[7..].trim().to_string(),
        _ => {
            return unauthorized(
                "missing_credentials",
                "Missing or malformed Authorization header.",
            )
        }
    };

    let store = state.admin_tokens.clone();
    let valid = match store.lock() {
        Ok(g) => g.contains(&token),
        Err(_) => false,
    };
    if !valid {
        return unauthorized("invalid_token", "Invalid or expired admin token.");
    }
    next.run(req).await
}

/// Login handler — issues a 64-hex-char random token when the password
/// matches. Returns 401 + `wrong_password` on mismatch.
pub async fn admin_login(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Response {
    if !state.cfg.admin_auth_enabled() {
        // Auth not enabled — still hand out a (valid, but pointless) token
        // so the SPA can keep a stable request shape in dev.
        return json_ok_token(&state);
    }
    let supplied = body
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !constant_time_eq(supplied.as_bytes(), state.cfg.admin_password.as_bytes()) {
        return json_response(
            StatusCode::UNAUTHORIZED,
            serde_json::json!({
                "error": {
                    "code": "wrong_password",
                    "message": "Password does not match.",
                    "type": "invalid_request_error",
                }
            }),
        );
    }
    json_ok_token(&state)
}

pub async fn admin_auth_status(State(state): State<AppState>) -> Response {
    let status = AuthStatus {
        auth_required: state.cfg.admin_auth_enabled(),
    };
    json_response(StatusCode::OK, serde_json::to_value(&status).unwrap())
}

fn json_ok_token(state: &AppState) -> Response {
    let mut bytes = [0u8; 32]; // 64 hex chars
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(&bytes);
    if let Ok(mut g) = state.admin_tokens.lock() {
        g.insert(token.clone());
    }
    json_response(
        StatusCode::OK,
        serde_json::json!({ "token": token }),
    )
}

fn json_response(status: StatusCode, body: serde_json::Value) -> Response {
    (status, Json(body)).into_response()
}

fn unauthorized(code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message,
            "type": "invalid_request_error",
        }
    });
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// In-memory token store. Wired into `AppState.admin_tokens` so middleware
/// and login handler share it cheaply.
pub type TokenStore = Arc<Mutex<HashSet<String>>>;

pub fn new_token_store() -> TokenStore {
    Arc::new(Mutex::new(HashSet::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}