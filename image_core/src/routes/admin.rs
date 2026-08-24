//! Admin API for the local management panel.

use crate::comfyui::ExecError;
use crate::logging::LogDraft;
use crate::mapping::detect_mapping;
use crate::models::{GenParams, LogEntry, ModelRecord};
use crate::routes::common::{json_response, server_error};
use crate::state::AppState;
use axum::extract::{Path, Query as AxumQuery, State};
use axum::http::StatusCode;
use axum::response::Response;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

// ----------------------------- Health -----------------------------

pub async fn health(State(state): State<AppState>) -> Response {
    let online = state.comfy.comfyui_online().await;
    let body = json!({
        "status": "ok",
        "comfyui_online": online,
        "version": env!("CARGO_PKG_VERSION"),
    });
    json_response(StatusCode::OK, body)
}

// ----------------------------- Overview -----------------------------

pub async fn overview(State(state): State<AppState>) -> Response {
    let pool = &state.pool;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM logs")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let start_of_day = chrono::Utc::now()
        .timestamp()
        - (chrono::Utc::now().timestamp() % 86400);
    let today: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM logs WHERE ts >= ?")
        .bind(start_of_day)
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let success: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM logs WHERE status = 'success'")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let error: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM logs WHERE status = 'error'")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let avg: f64 = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT AVG(latency_ms) FROM logs WHERE status='success'",
    )
    .fetch_one(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(0.0);
    let models_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let keys_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let comfyui_online = state.comfy.comfyui_online().await;

    json_response(
        StatusCode::OK,
        json!({
            "requests_total": total,
            "requests_today": today,
            "success_count": success,
            "error_count": error,
            "avg_latency_ms": avg,
            "models_count": models_count,
            "keys_count": keys_count,
            "comfyui_online": comfyui_online,
        }),
    )
}

// ----------------------------- Models -----------------------------

pub async fn list_models(State(state): State<AppState>) -> Response {
    match sqlx::query_as::<_, (String, String, i64, String, String, String, i64, i64)>(
        "SELECT id, name, enabled, kind, workflow, mapping, created_at, updated_at FROM models ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => {
            let out: Vec<Value> = rows
                .into_iter()
                .map(|(id, name, enabled, kind, workflow, mapping, created_at, updated_at)| {
                    let workflow_json: Value =
                        serde_json::from_str(&workflow).unwrap_or(Value::Null);
                    let mapping_json: Value =
                        serde_json::from_str(&mapping).unwrap_or(json!({}));
                    json!({
                        "id": id,
                        "name": name,
                        "enabled": enabled != 0,
                        "kind": kind,
                        "workflow": workflow_json,
                        "mapping": mapping_json,
                        "created_at": created_at,
                        "updated_at": updated_at,
                    })
                })
                .collect();
            json_response(StatusCode::OK, json!(out))
        }
        Err(e) => server_error(&e.to_string()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ImportBody {
    #[serde(default)]
    pub name: Option<String>,
    pub workflow: Value,
}

pub async fn import_model(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    // Accept either bare workflow JSON (no `workflow` key, has nodes) or wrapped {name, workflow}.
    let (name_opt, workflow_val) = if body.get("workflow").is_some() {
        match serde_json::from_value::<ImportBody>(body) {
            Ok(b) => (b.name, b.workflow),
            Err(e) => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    json!({"error": {"code":"invalid_body","message":e.to_string()}}),
                );
            }
        }
    } else {
        (None, body)
    };

    // Auto-convert UI format → API format if needed.
    let workflow_val = if crate::workflow_convert::is_ui_format(&workflow_val) {
        match crate::workflow_convert::ui_to_api(&workflow_val) {
            Ok(api) => api,
            Err(e) => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    json!({"error":{"code":"ui_conversion_failed","message":e}}),
                );
            }
        }
    } else {
        workflow_val
    };

    let detected = detect_mapping(&workflow_val);
    let detected_kind = crate::mapping::detect_kind(&workflow_val);

    let name = name_opt.unwrap_or_else(|| {
        format!(
            "model-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
        )
    });

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let workflow_s = serde_json::to_string(&workflow_val).unwrap_or("{}".to_string());
    let mapping_s = serde_json::to_string(&detected).unwrap_or("{}".to_string());

    let res = sqlx::query(
        "INSERT INTO models (id, name, enabled, kind, workflow, mapping, created_at, updated_at) VALUES (?, ?, 1, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(detected_kind)
    .bind(&workflow_s)
    .bind(&mapping_s)
    .bind(now)
    .bind(now)
    .execute(&state.pool)
    .await;

    if let Err(e) = res {
        // Unique name collision → retry with a suffix.
        if e.to_string().contains("UNIQUE") {
            let alt_name = format!("{name}-{}", &id[..8]);
            let res2 = sqlx::query(
                "INSERT INTO models (id, name, enabled, kind, workflow, mapping, created_at, updated_at) VALUES (?, ?, 1, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&alt_name)
            .bind(detected_kind)
            .bind(&workflow_s)
            .bind(&mapping_s)
            .bind(now)
            .bind(now)
            .execute(&state.pool)
            .await;
            if let Err(e2) = res2 {
                return server_error(&e2.to_string());
            }
        } else {
            return server_error(&e.to_string());
        }
    }

    json_response(
        StatusCode::CREATED,
        json!({
            "id": id,
            "name": name,
            "enabled": true,
            "kind": detected_kind,
            "workflow": workflow_val,
            "mapping": detected,
            "created_at": now,
            "updated_at": now,
        }),
    )
}

pub async fn get_model(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let row: Option<(
        String,
        String,
        i64,
        String,
        String,
        String,
        i64,
        i64,
    )> = sqlx::query_as(
        "SELECT id, name, enabled, kind, workflow, mapping, created_at, updated_at FROM models WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((id, name, enabled, kind, workflow, mapping, created_at, updated_at)) => {
            let wf: Value = serde_json::from_str(&workflow).unwrap_or(Value::Null);
            let mp: Value = serde_json::from_str(&mapping).unwrap_or(json!({}));
            json_response(
                StatusCode::OK,
                json!({
                    "id": id,
                    "name": name,
                    "enabled": enabled != 0,
                    "kind": kind,
                    "workflow": wf,
                    "mapping": mp,
                    "created_at": created_at,
                    "updated_at": updated_at,
                }),
            )
        }
        None => json_response(
            StatusCode::NOT_FOUND,
            json!({"error":{"code":"not_found","message":"model not found"}}),
        ),
    }
}

pub async fn update_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    let name = body.get("name").and_then(|v| v.as_str()).map(String::from);
    let enabled = body.get("enabled").and_then(|v| v.as_bool());
    let mapping = body.get("mapping").cloned();
    let workflow = body.get("workflow").cloned();

    let now = chrono::Utc::now().timestamp();
    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => return server_error(&e.to_string()),
    };

    if let Some(name) = name {
        if let Err(e) = sqlx::query("UPDATE models SET name = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(now)
            .bind(&id)
            .execute(&mut *tx)
            .await
        {
            return server_error(&e.to_string());
        }
    }
    if let Some(enabled) = enabled {
        if let Err(e) =
            sqlx::query("UPDATE models SET enabled = ?, updated_at = ? WHERE id = ?")
                .bind(if enabled { 1i64 } else { 0i64 })
                .bind(now)
                .bind(&id)
                .execute(&mut *tx)
                .await
        {
            return server_error(&e.to_string());
        }
    }
    if let Some(mapping) = mapping {
        let s = serde_json::to_string(&mapping).unwrap_or("{}".to_string());
        if let Err(e) = sqlx::query("UPDATE models SET mapping = ?, updated_at = ? WHERE id = ?")
            .bind(&s)
            .bind(now)
            .bind(&id)
            .execute(&mut *tx)
            .await
        {
            return server_error(&e.to_string());
        }
    }
    if let Some(workflow) = workflow {
        let s = serde_json::to_string(&workflow).unwrap_or("{}".to_string());
        if let Err(e) = sqlx::query("UPDATE models SET workflow = ?, updated_at = ? WHERE id = ?")
            .bind(&s)
            .bind(now)
            .bind(&id)
            .execute(&mut *tx)
            .await
        {
            return server_error(&e.to_string());
        }
    }

    if let Err(e) = tx.commit().await {
        return server_error(&e.to_string());
    }
    get_model(State(state), Path(id)).await
}

pub async fn delete_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match sqlx::query("DELETE FROM models WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => json_response(StatusCode::OK, json!({"deleted": id})),
        Ok(_) => json_response(
            StatusCode::NOT_FOUND,
            json!({"error":{"code":"not_found","message":"model not found"}}),
        ),
        Err(e) => server_error(&e.to_string()),
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TestBody {
    pub prompt: String,
    #[serde(default)]
    pub negative_prompt: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub seed: Option<i64>,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default)]
    pub cfg: Option<f64>,
}

pub async fn test_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    let body: TestBody = match serde_json::from_value(body) {
        Ok(b) => b,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({"error":{"code":"invalid_request","message":e.to_string()}}),
            );
        }
    };
    let model = match load_model_by_id(&state, &id).await {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let mut params = GenParams::default();
    params.prompt = body.prompt.clone();
    params.negative_prompt = body.negative_prompt.clone();
    if let Some(w) = body.width {
        params.width = Some(w);
    }
    if let Some(h) = body.height {
        params.height = Some(h);
    }
    params.seed = body.seed;
    params.steps = body.steps;
    params.cfg = body.cfg;

    let timer = crate::logging::timer();
    let mut draft = LogDraft::new("admin_test");
    draft.request_body = Some(serde_json::to_value(&body).unwrap_or(Value::Null));

    let (wf, final_seed) = state
        .comfy
        .build_workflow(&model.workflow, &model.mapping, &params);
    let timeout_s = state.cfg.request_timeout_s;
    let result = state
        .comfy
        .run_queued(&state.queue, &model.name, &body.prompt, &wf, final_seed, timeout_s)
        .await;
    let elapsed = timer.elapsed_ms();
    match result {
        Ok(paths) => {
            let urls: Vec<String> = paths
                .into_iter()
                .map(|p| {
                    let fname = std::path::Path::new(&p)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default();
                    format!("/files/images/{}", fname)
                })
                .collect();
            let resp_body = json!({
                "images": urls,
                "latency_ms": elapsed,
                "seed": final_seed,
            });
            draft.model = Some(model.name.clone());
            draft.prompt = Some(body.prompt.clone());
            draft.latency_ms = elapsed;
            draft.image_urls = urls;
            draft.seed = Some(final_seed);
            draft.response_status = Some(200);
            draft.commit(&state).await;
            json_response(StatusCode::OK, resp_body)
        }
        Err(e) => {
            let resp_body = json!({
                "error": {"code": "generation_failed", "message": e.to_string()},
                "latency_ms": elapsed,
            });
            draft.model = Some(model.name.clone());
            draft.prompt = Some(body.prompt.clone());
            draft.latency_ms = elapsed;
            draft.response_status = Some(500);
            draft = draft.error(e.to_string());
            draft.commit(&state).await;
            json_response(StatusCode::INTERNAL_SERVER_ERROR, resp_body)
        }
    }
}

async fn load_model_by_id(state: &AppState, id: &str) -> Result<ModelRecord, Response> {
    let row: Option<(
        String,
        String,
        i64,
        String,
        String,
        String,
        i64,
        i64,
    )> = sqlx::query_as(
        "SELECT id, name, enabled, kind, workflow, mapping, created_at, updated_at FROM models WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| server_error(&e.to_string()))?;

    let (id, name, enabled, kind, workflow, mapping, created_at, updated_at) = match row {
        Some(r) => r,
        None => {
            return Err(json_response(
                StatusCode::NOT_FOUND,
                json!({"error":{"code":"not_found","message":"model not found"}}),
            ));
        }
    };
    let wf: Value = serde_json::from_str(&workflow).unwrap_or(Value::Null);
    let mp: crate::models::ParamMapping = serde_json::from_str(&mapping).unwrap_or_default();
    Ok(ModelRecord {
        id,
        name,
        enabled: enabled != 0,
        kind,
        workflow: wf,
        mapping: mp,
        created_at,
        updated_at,
    })
}

// ----------------------------- Keys -----------------------------

pub async fn list_keys(State(state): State<AppState>) -> Response {
    let rows: Vec<(String, String, i64, i64)> = match sqlx::query_as(
        "SELECT id, name, enabled, created_at FROM api_keys ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return server_error(&e.to_string()),
    };
    let data: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, enabled, created_at)| {
            json!({
                "id": id,
                "name": name,
                "enabled": enabled != 0,
                "created_at": created_at,
            })
        })
        .collect();
    json_response(StatusCode::OK, json!(data))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateKeyBody {
    pub name: String,
}

#[axum::debug_handler]
pub async fn create_key(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    let parsed: CreateKeyBody = match serde_json::from_value(body) {
        Ok(p) => p,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({"error":{"code":"invalid_request","message":e.to_string()}}),
            );
        }
    };
    // Use OsRng (Send + Sync) to avoid ThreadRng's !Send leaking into the future.
    let id = uuid::Uuid::new_v4().to_string();
    let mut bytes = [0u8; 24];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let key = format!("sk-{}", hex::encode(&bytes));
    let now = chrono::Utc::now().timestamp();

    if let Err(e) = sqlx::query(
        "INSERT INTO api_keys (id, name, key, enabled, created_at) VALUES (?, ?, ?, 1, ?)",
    )
    .bind(&id)
    .bind(&parsed.name)
    .bind(&key)
    .bind(now)
    .execute(&state.pool)
    .await
    {
        return server_error(&e.to_string());
    }
    json_response(
        StatusCode::CREATED,
        json!({
            "id": id,
            "name": parsed.name,
            "key": key,
            "enabled": true,
            "created_at": now,
        }),
    )
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateKeyBody {
    pub enabled: bool,
}

pub async fn update_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    let parsed: UpdateKeyBody = match serde_json::from_value(body) {
        Ok(p) => p,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({"error":{"code":"invalid_request","message":e.to_string()}}),
            );
        }
    };
    let res = sqlx::query("UPDATE api_keys SET enabled = ? WHERE id = ?")
        .bind(if parsed.enabled { 1i64 } else { 0i64 })
        .bind(&id)
        .execute(&state.pool)
        .await;
    match res {
        Ok(r) if r.rows_affected() > 0 => json_response(StatusCode::OK, json!({"updated": id})),
        Ok(_) => json_response(
            StatusCode::NOT_FOUND,
            json!({"error":{"code":"not_found","message":"key not found"}}),
        ),
        Err(e) => server_error(&e.to_string()),
    }
}

pub async fn delete_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    match sqlx::query("DELETE FROM api_keys WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await
    {
        Ok(r) if r.rows_affected() > 0 => json_response(StatusCode::OK, json!({"deleted": id})),
        Ok(_) => json_response(
            StatusCode::NOT_FOUND,
            json!({"error":{"code":"not_found","message":"key not found"}}),
        ),
        Err(e) => server_error(&e.to_string()),
    }
}

// ----------------------------- Logs -----------------------------

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_page_size")]
    pub page_size: u64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

fn default_page() -> u64 { 1 }
fn default_page_size() -> u64 { 20 }

pub async fn list_logs(
    State(state): State<AppState>,
    AxumQuery(q): AxumQuery<LogQuery>,
) -> Response {
    let page = q.page.max(1);
    let page_size = q.page_size.clamp(1, 200);
    let offset = (page - 1) * page_size;

    let mut where_clauses: Vec<String> = Vec::new();
    if q.status.is_some() {
        where_clauses.push("status = ?".into());
    }
    if q.model.is_some() {
        where_clauses.push("model = ?".into());
    }
    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM logs {}", where_sql);
    let list_sql = format!(
        "SELECT id, ts, provider, key_name, model, prompt, size, seed, status, error, latency_ms, image_urls, request_body, response_status FROM logs {} ORDER BY ts DESC LIMIT ? OFFSET ?",
        where_sql
    );

    let total: i64 = {
        let mut query = sqlx::query_scalar::<_, i64>(&count_sql).bind(0i64);
        if let Some(s) = &q.status {
            query = query.bind(s);
        }
        if let Some(m) = &q.model {
            query = query.bind(m);
        }
        query.fetch_one(&state.pool).await.unwrap_or(0)
    };

    let mut list_query = sqlx::query_as::<_, LogRow>(&list_sql);
    if let Some(s) = &q.status {
        list_query = list_query.bind(s);
    }
    if let Some(m) = &q.model {
        list_query = list_query.bind(m);
    }
    list_query = list_query.bind(page_size as i64).bind(offset as i64);
    let rows = match list_query.fetch_all(&state.pool).await {
        Ok(r) => r,
        Err(e) => return server_error(&e.to_string()),
    };

    let items: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "ts": r.ts,
                "provider": r.provider,
                "key_name": r.key_name,
                "model": r.model,
                "prompt": r.prompt,
                "size": r.size,
                "seed": r.seed,
                "status": r.status,
                "error": r.error,
                "latency_ms": r.latency_ms,
                "image_urls": r.image_urls,
                "request_body": r.request_body,
                "response_status": r.response_status,
            })
        })
        .collect();

    json_response(
        StatusCode::OK,
        json!({
            "items": items,
            "total": total,
            "page": page,
            "page_size": page_size,
        }),
    )
}

pub async fn get_log(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let row: Option<LogRow> = match sqlx::query_as(
        "SELECT id, ts, provider, key_name, model, prompt, size, seed, status, error, latency_ms, image_urls, request_body, response_status FROM logs WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => return server_error(&e.to_string()),
    };

    match row {
        Some(r) => {
            let body: LogEntry = LogEntry {
                id: r.id,
                ts: r.ts,
                provider: r.provider,
                key_name: r.key_name,
                model: r.model,
                prompt: r.prompt,
                size: r.size,
                seed: r.seed,
                status: r.status,
                error: r.error,
                latency_ms: r.latency_ms,
                image_urls: r.image_urls,
                request_body: r.request_body,
                response_status: r.response_status,
            };
            json_response(StatusCode::OK, serde_json::to_value(&body).unwrap())
        }
        None => json_response(
            StatusCode::NOT_FOUND,
            json!({"error":{"code":"not_found","message":"log not found"}}),
        ),
    }
}

pub async fn clear_logs(State(state): State<AppState>) -> Response {
    if let Err(e) = sqlx::query("DELETE FROM logs").execute(&state.pool).await {
        return server_error(&e.to_string());
    }
    json_response(StatusCode::OK, json!({"cleared": true}))
}

#[derive(sqlx::FromRow)]
struct LogRow {
    id: String,
    ts: i64,
    provider: String,
    key_name: Option<String>,
    model: Option<String>,
    prompt: Option<String>,
    size: Option<String>,
    seed: Option<i64>,
    status: String,
    error: Option<String>,
    latency_ms: i64,
    image_urls: Option<String>,
    request_body: Option<String>,
    response_status: Option<i64>,
}

// ----------------------------- Settings -----------------------------

pub async fn get_settings(State(state): State<AppState>) -> Response {
    // Table-wins over env defaults — read every key from settings, fall back to
    // the live Config (env + .env + built-in defaults).
    let mut s = crate::models::Settings {
        comfyui_url: state.cfg.comfyui_url.clone(),
        request_timeout_s: state.cfg.request_timeout_s,
        listen_addr: state.cfg.listen_addr.clone(),
        data_dir: state.cfg.data_dir.clone(),
        max_concurrent: state.cfg.max_concurrent,
        image_retention_hours: state.cfg.image_retention_hours,
        image_max_total_mb: state.cfg.image_max_total_mb,
    };

    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
    for (k, v) in rows {
        match k.as_str() {
            "comfyui_url" => s.comfyui_url = v,
            "request_timeout_s" => s.request_timeout_s = v.parse().unwrap_or(s.request_timeout_s),
            "listen_addr" => s.listen_addr = v,
            "data_dir" => s.data_dir = v,
            "max_concurrent" => s.max_concurrent = v.parse().unwrap_or(s.max_concurrent),
            "image_retention_hours" => {
                s.image_retention_hours = v.parse().unwrap_or(s.image_retention_hours)
            }
            "image_max_total_mb" => {
                s.image_max_total_mb = v.parse().unwrap_or(s.image_max_total_mb)
            }
            _ => {}
        }
    }

    json_response(StatusCode::OK, serde_json::to_value(&s).unwrap())
}

pub async fn update_settings(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    // Fields the table can hold. Anything not in this list is rejected.
    let accepted: &[&str] = &[
        "comfyui_url",
        "request_timeout_s",
        "listen_addr",
        "data_dir",
        "max_concurrent",
        "image_retention_hours",
        "image_max_total_mb",
    ];

    let mut restart_required: Vec<&'static str> = Vec::new();

    let pairs: Vec<(&'static str, String, bool)> = vec![
        ("comfyui_url", body.get("comfyui_url").and_then(|v| v.as_str()).map(String::from), false),
        ("request_timeout_s", body.get("request_timeout_s").and_then(|v| v.as_u64()).map(|v| v.to_string()), false),
        ("listen_addr", body.get("listen_addr").and_then(|v| v.as_str()).map(String::from), true),
        ("data_dir", body.get("data_dir").and_then(|v| v.as_str()).map(String::from), true),
        ("max_concurrent", body.get("max_concurrent").and_then(|v| v.as_u64()).map(|v| v.to_string()), false),
        ("image_retention_hours", body.get("image_retention_hours").and_then(|v| v.as_u64()).map(|v| v.to_string()), false),
        ("image_max_total_mb", body.get("image_max_total_mb").and_then(|v| v.as_u64()).map(|v| v.to_string()), false),
    ]
    .into_iter()
    .filter_map(|(k, v, r)| v.map(|val| (k, val, r)))
    .collect();

    for (k, v, needs_restart) in &pairs {
        if !accepted.contains(k) {
            continue;
        }
        if let Err(e) = sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(k)
        .bind(v)
        .execute(&state.pool)
        .await
        {
            return server_error(&e.to_string());
        }
        if *needs_restart {
            restart_required.push(*k);
        }
    }

    // Hot-apply: max_concurrent rebuilds the semaphore live.
    if let Some(v) = body.get("max_concurrent").and_then(|x| x.as_u64()) {
        state.queue.set_max(v as u32).await;
    }
    // Hot-apply: image_retention_hours / image_max_total_mb take effect on
    // the next cleanup tick (we don't manually trigger here — too aggressive).

    if !restart_required.is_empty() {
        return json_response(
            StatusCode::OK,
            json!({
                "ok": true,
                "restart_required": restart_required,
            }),
        );
    }

    json_response(StatusCode::OK, json!({"ok": true}))
}

// ----------------------------- Queue -----------------------------

pub async fn get_queue(State(state): State<AppState>) -> Response {
    let snap = state.queue.snapshot().await;
    json_response(StatusCode::OK, serde_json::to_value(&snap).unwrap())
}

// ----------------------------- Storage -----------------------------

pub async fn get_storage(State(state): State<AppState>) -> Response {
    let stats = crate::cleanup::storage_stats(&state).await;
    json_response(StatusCode::OK, serde_json::to_value(&stats).unwrap())
}

pub async fn cleanup_storage(State(state): State<AppState>) -> Response {
    let result = crate::cleanup::run_cleanup(&state).await;
    json_response(StatusCode::OK, serde_json::to_value(&result).unwrap())
}

// ----------------------------- Import dir -----------------------------

#[derive(Debug, Deserialize)]
pub struct ImportDirBody {
    #[serde(default)]
    pub path: Option<String>,
}

/// Walk a directory for `*.json` workflow files, convert UI → API on the fly,
/// detect mapping + kind, and insert each as a new model.
pub async fn import_dir(
    State(state): State<AppState>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    let parsed: ImportDirBody = match serde_json::from_value(body) {
        Ok(p) => p,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({"error":{"code":"invalid_body","message":e.to_string()}}),
            );
        }
    };
    let path = match parsed.path {
        Some(p) if !p.trim().is_empty() => p.trim().to_string(),
        _ => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({"error":{"code":"missing_path","message":"`path` is required"}}),
            );
        }
    };

    let entries = match tokio::fs::read_dir(&path).await {
        Ok(e) => e,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({"error":{"code":"bad_path","message":format!("cannot read directory: {e}")}}),
            );
        }
    };

    let mut imported: Vec<Value> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();

    let mut entries = entries;
    loop {
        let entry = match entries.next_entry().await {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(_) => continue,
        };
        let file_path = entry.path();
        if !file_path.is_file() {
            continue;
        }
        let is_json = file_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        if !is_json {
            continue;
        }
        let display_name = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Skip already-imported same-name model.
        let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM models WHERE name = ?")
            .bind(&display_name.trim_end_matches(".json"))
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
        if existing.is_some() {
            skipped.push(json!({
                "file": display_name,
                "reason": "model with this name already exists"
            }));
            continue;
        }

        // Read file as bytes (UTF-8 lossy is fine for workflow JSON, even if
        // the file is non-ASCII like Chinese filenames).
        let bytes = match tokio::fs::read(&file_path).await {
            Ok(b) => b,
            Err(e) => {
                skipped.push(json!({
                    "file": display_name,
                    "reason": format!("read error: {e}")
                }));
                continue;
            }
        };
        let text = String::from_utf8_lossy(&bytes);
        let raw: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                skipped.push(json!({
                    "file": display_name,
                    "reason": format!("parse error: {e}")
                }));
                continue;
            }
        };

        // Auto-convert UI format → API format.
        let workflow_val = if crate::workflow_convert::is_ui_format(&raw) {
            match crate::workflow_convert::ui_to_api(&raw) {
                Ok(api) => api,
                Err(e) => {
                    skipped.push(json!({
                        "file": display_name,
                        "reason": format!("ui→api conversion failed: {e}")
                    }));
                    continue;
                }
            }
        } else {
            raw
        };

        let mapping = detect_mapping(&workflow_val);
        let kind = crate::mapping::detect_kind(&workflow_val);
        let name = display_name.trim_end_matches(".json").to_string();
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let wf_s = serde_json::to_string(&workflow_val).unwrap_or("{}".into());
        let mp_s = serde_json::to_string(&mapping).unwrap_or("{}".into());

        let res = sqlx::query(
            "INSERT INTO models (id, name, enabled, kind, workflow, mapping, created_at, updated_at) VALUES (?, ?, 1, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(kind)
        .bind(&wf_s)
        .bind(&mp_s)
        .bind(now)
        .bind(now)
        .execute(&state.pool)
        .await;

        if let Err(e) = res {
            skipped.push(json!({
                "file": display_name,
                "reason": format!("db insert: {e}")
            }));
            continue;
        }
        imported.push(json!({
            "id": id,
            "name": name,
            "kind": kind,
        }));
    }

    json_response(
        StatusCode::OK,
        json!({
            "imported": imported,
            "skipped": skipped,
        }),
    )
}

// ----------------------------- Stats -----------------------------

#[derive(Debug, Deserialize)]
pub struct StatsSeriesQuery {
    /// Time window in hours (default 24).
    pub hours: Option<i64>,
    /// Bucket size — `hour` (default) or `day`.
    pub bucket: Option<String>,
    /// Optional API key name filter (matches logs.key_name).
    pub key_id: Option<String>,
    /// Optional model id filter (matches logs.model).
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
struct SeriesPoint {
    ts: i64,
    total: i64,
    success: i64,
    error: i64,
    avg_latency_ms: f64,
}

pub async fn stats_series(
    State(state): State<AppState>,
    AxumQuery(q): AxumQuery<StatsSeriesQuery>,
) -> Response {
    let hours = q.hours.unwrap_or(24).clamp(1, 24 * 365);
    let bucket = q.bucket.as_deref().unwrap_or("hour");
    let bucket_secs: i64 = match bucket {
        "day" => 86400,
        _ => 3600,
    };
    let now = chrono::Utc::now().timestamp();
    let since = now - hours * 3600;
    // Floor `since` to bucket boundary.
    let first_bucket = (since / bucket_secs) * bucket_secs;
    let last_bucket = (now / bucket_secs) * bucket_secs;

    // Build WHERE clauses.
    let mut wh: Vec<String> = vec![format!("ts >= {}", since)];
    if let Some(k) = &q.key_id {
        if !k.is_empty() {
            wh.push("key_name = ?".into());
        }
    }
    if let Some(m) = &q.model {
        if !m.is_empty() {
            wh.push("model = ?".into());
        }
    }
    let where_sql = wh.join(" AND ");

    let sql = format!(
        "SELECT (ts / {b}) * {b} AS bucket_ts, \
                COUNT(*) AS total, \
                SUM(CASE WHEN status='success' THEN 1 ELSE 0 END) AS succ, \
                SUM(CASE WHEN status='error'   THEN 1 ELSE 0 END) AS err, \
                AVG(latency_ms) AS avg_lat \
         FROM logs \
         WHERE {w} \
         GROUP BY bucket_ts \
         ORDER BY bucket_ts ASC",
        b = bucket_secs,
        w = where_sql,
    );
    let mut query = sqlx::query_as::<_, (i64, i64, i64, i64, Option<f64>)>(&sql);
    if let Some(k) = &q.key_id {
        if !k.is_empty() {
            query = query.bind(k);
        }
    }
    if let Some(m) = &q.model {
        if !m.is_empty() {
            query = query.bind(m);
        }
    }
    let rows = match query.fetch_all(&state.pool).await {
        Ok(r) => r,
        Err(e) => return server_error(&e.to_string()),
    };

    let mut by_bucket: std::collections::HashMap<i64, (i64, i64, i64, f64)> =
        std::collections::HashMap::new();
    for (b, total, succ, err, avg) in rows {
        by_bucket.insert(b, (total, succ, err, avg.unwrap_or(0.0)));
    }

    let mut points: Vec<SeriesPoint> = Vec::new();
    let mut ts = first_bucket;
    while ts <= last_bucket {
        if let Some(&(t, s, e, avg)) = by_bucket.get(&ts) {
            points.push(SeriesPoint { ts, total: t, success: s, error: e, avg_latency_ms: avg });
        } else {
            points.push(SeriesPoint { ts, total: 0, success: 0, error: 0, avg_latency_ms: 0.0 });
        }
        ts += bucket_secs;
    }

    json_response(StatusCode::OK, json!({ "points": points }))
}

pub async fn stats_keys(State(state): State<AppState>) -> Response {
    // We join on logs.key_name = api_keys.name, but logs can also reference
    // keys that have since been deleted. Surface those with key_id == null.
    let rows = sqlx::query_as::<_, (String, String, i64, i64, i64, Option<f64>, Option<i64>)>(
        "SELECT k.id, k.name, \
                COUNT(l.id) AS total, \
                COALESCE(SUM(CASE WHEN l.status='success' THEN 1 ELSE 0 END), 0) AS succ, \
                COALESCE(SUM(CASE WHEN l.status='error'   THEN 1 ELSE 0 END), 0) AS err, \
                AVG(l.latency_ms) AS avg_lat, \
                MAX(l.ts) AS last_ts \
         FROM api_keys k \
         LEFT JOIN logs l ON l.key_name = k.name \
         GROUP BY k.id, k.name \
         ORDER BY total DESC, k.name ASC",
    )
    .fetch_all(&state.pool)
    .await;

    let mut out: Vec<Value> = Vec::new();
    match rows {
        Ok(rs) => {
            for (id, name, total, succ, err, avg, last_ts) in rs {
                out.push(json!({
                    "key_id": id,
                    "key_name": name,
                    "total": total,
                    "success": succ,
                    "error": err,
                    "avg_latency_ms": avg.unwrap_or(0.0),
                    "last_used_at": last_ts,
                }));
            }
            // Also surface "orphan" key_name values from logs that no longer
            // exist in api_keys.
            let orphans = sqlx::query_as::<_, (String, i64, i64, i64, Option<f64>, Option<i64>)>(
                "SELECT key_name, \
                        COUNT(*) AS total, \
                        SUM(CASE WHEN status='success' THEN 1 ELSE 0 END) AS succ, \
                        SUM(CASE WHEN status='error'   THEN 1 ELSE 0 END) AS err, \
                        AVG(latency_ms) AS avg_lat, \
                        MAX(ts) AS last_ts \
                 FROM logs \
                 WHERE key_name IS NOT NULL \
                   AND key_name != '' \
                   AND key_name NOT IN (SELECT name FROM api_keys) \
                 GROUP BY key_name \
                 ORDER BY total DESC",
            )
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
            for (name, total, succ, err, avg, last_ts) in orphans {
                out.push(json!({
                    "key_id": Value::Null,
                    "key_name": name,
                    "total": total,
                    "success": succ,
                    "error": err,
                    "avg_latency_ms": avg.unwrap_or(0.0),
                    "last_used_at": last_ts,
                }));
            }
        }
        Err(e) => return server_error(&e.to_string()),
    }
    json_response(StatusCode::OK, json!(out))
}

pub async fn stats_models(State(state): State<AppState>) -> Response {
    let rows = sqlx::query_as::<_, (String, String, i64, i64, i64, Option<f64>)>(
        "SELECT m.id, m.name, \
                COUNT(l.id) AS total, \
                COALESCE(SUM(CASE WHEN l.status='success' THEN 1 ELSE 0 END), 0) AS succ, \
                COALESCE(SUM(CASE WHEN l.status='error'   THEN 1 ELSE 0 END), 0) AS err, \
                AVG(l.latency_ms) AS avg_lat \
         FROM models m \
         LEFT JOIN logs l ON l.model = m.name \
         GROUP BY m.id, m.name \
         ORDER BY total DESC, m.name ASC",
    )
    .fetch_all(&state.pool)
    .await;

    let mut out: Vec<Value> = Vec::new();
    match rows {
        Ok(rs) => {
            for (id, name, total, succ, err, avg) in rs {
                out.push(json!({
                    "model_id": id,
                    "model_name": name,
                    "total": total,
                    "success": succ,
                    "error": err,
                    "avg_latency_ms": avg.unwrap_or(0.0),
                }));
            }
            // Orphans (logs reference models that no longer exist).
            let orphans = sqlx::query_as::<_, (String, i64, i64, i64, Option<f64>)>(
                "SELECT model, \
                        COUNT(*) AS total, \
                        SUM(CASE WHEN status='success' THEN 1 ELSE 0 END) AS succ, \
                        SUM(CASE WHEN status='error'   THEN 1 ELSE 0 END) AS err, \
                        AVG(latency_ms) AS avg_lat \
                 FROM logs \
                 WHERE model IS NOT NULL \
                   AND model != '' \
                   AND model NOT IN (SELECT name FROM models) \
                 GROUP BY model \
                 ORDER BY total DESC",
            )
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
            for (name, total, succ, err, avg) in orphans {
                out.push(json!({
                    "model_id": Value::Null,
                    "model_name": name,
                    "total": total,
                    "success": succ,
                    "error": err,
                    "avg_latency_ms": avg.unwrap_or(0.0),
                }));
            }
        }
        Err(e) => return server_error(&e.to_string()),
    }
    json_response(StatusCode::OK, json!(out))
}

// ----------------------------- helpers -----------------------------

// Silence unused-import warnings on items used only by other route modules.
#[allow(dead_code)]
fn _silence_exec(e: &ExecError) {
    let _ = e;
}
#[allow(dead_code)]
fn _silence_find(model: &str) {
    let _ = model;
}
#[allow(dead_code)]
fn _silence_dur() {
    let _ = Duration::from_secs(0);
}