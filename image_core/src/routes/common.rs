//! Shared helpers used by every public route.

use crate::comfyui::{ComfyExecutor, ExecError};
use crate::logging::LogDraft;
use crate::models::{GenParams, ModelRecord};
use crate::state::AppState;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use serde_json::{json, Value};

/// Find a model by id OR name, must be enabled. Returns the full record.
pub async fn find_enabled_model(state: &AppState, identifier: &str) -> Result<ModelRecord, Response> {
    let row: Option<ModelRow> = sqlx::query_as(
        "SELECT id, name, enabled, kind, workflow, mapping FROM models WHERE (id = ? OR name = ?) AND enabled = 1",
    )
    .bind(identifier)
    .bind(identifier)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| server_error(&e.to_string()))?;

    match row {
        Some(r) => {
            let workflow: Value = serde_json::from_str(&r.workflow).unwrap_or(Value::Null);
            let mapping: crate::models::ParamMapping =
                serde_json::from_str(&r.mapping).unwrap_or_default();
            Ok(ModelRecord {
                id: r.id,
                name: r.name,
                enabled: r.enabled != 0,
                kind: r.kind,
                workflow,
                mapping,
                created_at: 0,
                updated_at: 0,
            })
        }
        None => Err(not_found_error("model_not_found", "The model does not exist or is disabled")),
    }
}

#[derive(sqlx::FromRow)]
struct ModelRow {
    id: String,
    name: String,
    enabled: i64,
    kind: String,
    workflow: String,
    mapping: String,
}

/// Execute a model with normalized params. Goes through the concurrency
/// queue. Returns (image_urls, seed_used).
pub async fn run_model(
    state: &AppState,
    comfy: &ComfyExecutor,
    model: &ModelRecord,
    params: &GenParams,
) -> Result<(Vec<String>, i64), ExecError> {
    let (workflow, final_seed) = comfy.build_workflow(&model.workflow, &model.mapping, params);
    let timeout_s = effective_timeout_s(state).await;
    let paths = comfy
        .run_queued(&state.queue, &model.name, &params.prompt, &workflow, final_seed, timeout_s)
        .await?;
    let urls = paths
        .into_iter()
        .map(|p| {
            let filename = std::path::Path::new(&p)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            format!("/files/images/{}", filename)
        })
        .collect();
    Ok((urls, final_seed))
}

async fn effective_timeout_s(state: &AppState) -> u64 {
    let stored: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = 'request_timeout_s'")
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
    stored
        .and_then(|(s,)| s.parse::<u64>().ok())
        .unwrap_or(state.cfg.request_timeout_s)
}

/// Build an HTTP response with given status + JSON body.
pub fn json_response(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

pub fn server_error(msg: &str) -> Response {
    json_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({"error": {"code": "internal_error", "message": msg}}),
    )
}

pub fn not_found_error(code: &str, msg: &str) -> Response {
    json_response(
        StatusCode::NOT_FOUND,
        json!({"error": {"code": code, "message": msg}}),
    )
}

pub fn bad_request_error(code: &str, msg: &str) -> Response {
    json_response(
        StatusCode::BAD_REQUEST,
        json!({"error": {"code": code, "message": msg}}),
    )
}

/// Convert URL list to b64_json list if response_format=b64_json.
#[allow(dead_code)]
pub fn maybe_b64(urls: Vec<String>, response_format: Option<&str>) -> Vec<Value> {
    match response_format {
        Some("b64_json") => urls
            .into_iter()
            .map(|_u| json!({"url": Value::Null}))
            .collect(),
        _ => urls.into_iter().map(|u| json!({"url": u})).collect(),
    }
}

/// Encode a file at the given path as base64.
pub fn file_to_base64(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Parse "WxH" or known aliases into (width, height).
pub fn parse_size(size: Option<&str>) -> Result<(u32, u32), String> {
    let s = size.unwrap_or("1024x1024");
    let lower = s.to_ascii_lowercase();
    if let Some((w, h)) = lower.split_once('x') {
        let w: u32 = w.parse().map_err(|_| format!("invalid size: {s}"))?;
        let h: u32 = h.parse().map_err(|_| format!("invalid size: {s}"))?;
        return Ok((w, h));
    }
    match lower.as_str() {
        "adaptive" => Ok((1024, 1024)),
        "1k" => Ok((1024, 1024)),
        "2k" => Ok((2048, 2048)),
        "4k" => Ok((4096, 4096)),
        "square" | "square_hd" => Ok((1024, 1024)),
        "portrait_4_3" => Ok((832, 1216)),
        "portrait_16_9" => Ok((768, 1360)),
        "landscape_4_3" => Ok((1216, 832)),
        "landscape_16_9" => Ok((1360, 768)),
        _ => Err(format!("unrecognized size token: {s}")),
    }
}

/// Apply prompt + optional extras to a draft log row.
pub fn apply_draft_basics(
    draft: LogDraft,
    model: &str,
    prompt: Option<&str>,
    size: Option<String>,
    seed: Option<i64>,
    request_body: Option<Value>,
    key_name: Option<String>,
    elapsed_ms: i64,
    urls: Vec<String>,
    response_status: u16,
    error: Option<String>,
) -> LogDraft {
    let mut d = draft;
    d.key_name = key_name;
    d.model = Some(model.to_string());
    d.prompt = prompt.map(|s| s.to_string());
    d.size = size;
    d.seed = seed;
    d.latency_ms = elapsed_ms;
    d.image_urls = urls;
    d.request_body = request_body;
    d.response_status = Some(response_status as i64);
    if let Some(e) = error {
        d = d.error(e);
    }
    d
}

#[allow(unused_imports)]
pub use crate::logging::timer;