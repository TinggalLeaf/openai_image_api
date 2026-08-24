//! Route groups: shared helpers + 4 public formats + admin + static files.

pub mod admin;
pub mod common;
pub mod public_openai;
pub mod public_siliconflow;
pub mod public_volcengine;
pub mod public_zhipu;

use crate::state::AppState;
use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};

pub fn build_router(state: AppState) -> Router {
    let public_openai = Router::new()
        .route(
            "/v1/images/generations",
            axum::routing::post(public_openai::create_image),
        )
        .route(
            "/openai/v1/images/generations",
            axum::routing::post(public_openai::create_image),
        )
        .route(
            "/v1/images/edits",
            axum::routing::post(public_openai::create_image_edit),
        )
        .route(
            "/v1/images/variations",
            axum::routing::post(public_openai::create_image_variation),
        )
        .route("/v1/models", axum::routing::get(public_openai::list_models))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::apikey_auth::require_api_key,
        ));

    let public_volcengine = Router::new()
        .route(
            "/api/v3/images/generations",
            axum::routing::post(public_volcengine::create_image),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::apikey_auth::require_api_key,
        ));

    let public_siliconflow = Router::new()
        .route(
            "/v1/images/generations",
            axum::routing::post(public_siliconflow::create_image),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::apikey_auth::require_api_key,
        ));

    let public_zhipu = Router::new()
        .route(
            "/paas/v4/images/generations",
            axum::routing::post(public_zhipu::create_image),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::apikey_auth::require_api_key,
        ));

    // Public admin endpoints (no auth required even when ADMIN_PASSWORD is set).
    let admin_public = Router::new()
        .route("/auth/status", axum::routing::get(crate::auth::admin_auth_status))
        .route("/login", axum::routing::post(crate::auth::admin_login));

    let admin_protected = Router::new()
        .route("/health", axum::routing::get(admin::health))
        .route("/overview", axum::routing::get(admin::overview))
        .route("/queue", axum::routing::get(admin::get_queue))
        .route("/models", axum::routing::get(admin::list_models))
        .route(
            "/models/import",
            axum::routing::post(admin::import_model),
        )
        .route(
            "/models/import-dir",
            axum::routing::post(admin::import_dir),
        )
        .route("/models/{id}", axum::routing::get(admin::get_model))
        .route("/models/{id}", axum::routing::put(admin::update_model))
        .route(
            "/models/{id}",
            axum::routing::delete(admin::delete_model),
        )
        .route(
            "/models/{id}/test",
            axum::routing::post(admin::test_model),
        )
        .route("/keys", axum::routing::get(admin::list_keys))
        .route("/keys", axum::routing::post(admin::create_key))
        .route("/keys/{id}", axum::routing::patch(admin::update_key))
        .route("/keys/{id}", axum::routing::delete(admin::delete_key))
        .route("/logs", axum::routing::get(admin::list_logs))
        .route("/logs", axum::routing::delete(admin::clear_logs))
        .route("/logs/{id}", axum::routing::get(admin::get_log))
        .route("/settings", axum::routing::get(admin::get_settings))
        .route("/settings", axum::routing::put(admin::update_settings))
        .route("/storage", axum::routing::get(admin::get_storage))
        .route(
            "/storage/cleanup",
            axum::routing::post(admin::cleanup_storage),
        )
        .route("/stats/series", axum::routing::get(admin::stats_series))
        .route("/stats/keys", axum::routing::get(admin::stats_keys))
        .route("/stats/models", axum::routing::get(admin::stats_models))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_admin_token,
        ));

    let admin = admin_public.merge(admin_protected);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .nest("/admin/api", admin)
        .nest("/volcengine", public_volcengine)
        .nest("/siliconflow", public_siliconflow)
        .nest("/zhipu", public_zhipu)
        .merge(public_openai)
        .route("/files/images/{filename}", get(serve_image))
        .fallback(spa_fallback)
        .layer(cors)
        .with_state(state)
}

/// SPA fallback — serve embedded static assets, falling back to
/// `index.html` so client-side routers work for any non-API path.
async fn spa_fallback(axum::extract::OriginalUri(uri): axum::extract::OriginalUri) -> Response {
    let path = uri.path();
    if let Some((bytes, mime)) = crate::embedded::read(path) {
        let status = if path == "/" || mime.starts_with("text/html") {
            StatusCode::OK
        } else {
            StatusCode::OK
        };
        return Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(bytes))
            .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "").into_response());
    }
    // Embedded lookup itself failed (very unusual — should always have index.html).
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        Body::from("asset not found"),
    )
        .into_response()
}

async fn serve_image(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(filename): Path<String>,
) -> Response {
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return (StatusCode::BAD_REQUEST, "invalid filename").into_response();
    }
    let mut path = PathBuf::from(&state.cfg.images_dir);
    path.push(&filename);
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mime = guess_mime(&filename);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(bytes))
                .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "").into_response())
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn guess_mime(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else {
        "application/octet-stream"
    }
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}