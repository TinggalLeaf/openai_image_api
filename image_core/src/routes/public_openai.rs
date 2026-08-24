//! OpenAI DALL·E compatible image generation + model listing.

use crate::logging::LogDraft;
use crate::models::GenParams;
use crate::routes::common::{
    apply_draft_basics, bad_request_error, file_to_base64, find_enabled_model, json_response,
    parse_size, run_model,
};
use crate::state::AppState;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Extension;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct CreateImageReq {
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub n: Option<u32>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub response_format: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    // Extensions
    #[serde(default)]
    pub negative_prompt: Option<String>,
    #[serde(default)]
    pub seed: Option<i64>,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default)]
    pub cfg: Option<f64>,
    #[serde(default)]
    pub sampler: Option<String>,
    #[serde(default)]
    pub scheduler: Option<String>,
}

pub async fn create_image(
    State(state): State<AppState>,
    Extension(auth): Extension<crate::apikey_auth::AuthedKey>,
    axum::Json(req): axum::Json<Value>,
) -> Response {
    let parsed: CreateImageReq = match serde_json::from_value(req.clone()) {
        Ok(p) => p,
        Err(e) => {
            return bad_request_error("invalid_request", &format!("Invalid body: {e}"));
        }
    };

    let n = parsed.n.unwrap_or(1).max(1);
    let (width, height) = match parse_size(parsed.size.as_deref()) {
        Ok(s) => s,
        Err(e) => return bad_request_error("invalid_size", &e),
    };
    let response_format = parsed.response_format.as_deref().unwrap_or("url");

    let model = match find_enabled_model(&state, &parsed.model).await {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let timer = crate::logging::timer();
    let mut draft = LogDraft::new("openai");
    draft.request_body = Some(req.clone());

    let mut params = GenParams::default();
    params.prompt = parsed.prompt.clone();
    params.negative_prompt = parsed.negative_prompt.clone();
    params.width = Some(width);
    params.height = Some(height);
    params.steps = parsed.steps;
    params.cfg = parsed.cfg;
    params.seed = parsed.seed;
    params.sampler = parsed.sampler.clone();
    params.scheduler = parsed.scheduler.clone();
    params.batch_size = Some(n);

    let result = run_model(&state, &state.comfy, &model, &params).await;
    let elapsed = timer.elapsed_ms();
    match result {
        Ok((urls, seed_used)) => {
            // If n>1, repeat urls (ComfyUI already produces batch_size images, returned as multiple files).
            let data = build_data_items(&urls, response_format, &state);
            let body = json!({
                "created": chrono::Utc::now().timestamp(),
                "data": data,
            });
            let draft = apply_draft_basics(
                draft,
                &parsed.model,
                Some(&parsed.prompt),
                Some(format!("{width}x{height}")),
                Some(seed_used),
                Some(req),
                Some(auth.name),
                elapsed,
                urls,
                200,
                None,
            );
            draft.commit(&state).await;
            json_response(StatusCode::OK, body)
        }
        Err(e) => {
            let draft = apply_draft_basics(
                draft,
                &parsed.model,
                Some(&parsed.prompt),
                Some(format!("{width}x{height}")),
                parsed.seed,
                Some(req),
                Some(auth.name),
                elapsed,
                Vec::new(),
                500,
                Some(e.to_string()),
            );
            draft.commit(&state).await;
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error":{"code":"generation_failed","message":e.to_string()}}),
            )
        }
    }
}

fn build_data_items(urls: &[String], response_format: &str, state: &AppState) -> Vec<Value> {
    urls.iter()
        .map(|u| {
            let filename = u.trim_start_matches("/files/images/");
            let abs = std::path::Path::new(&state.cfg.images_dir).join(filename);
            if response_format == "b64_json" {
                json!({"b64_json": file_to_base64(abs.to_str().unwrap_or("")).unwrap_or_default()})
            } else {
                json!({"url": u})
            }
        })
        .collect()
}

pub async fn list_models(State(state): State<AppState>) -> Response {
    let rows: Vec<(String, String, String, i64)> = match sqlx::query_as(
        "SELECT id, name, kind, created_at FROM models WHERE enabled = 1 ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error":{"code":"db_error","message":e.to_string()}}),
            );
        }
    };

    let data: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, kind, created_at)| {
            json!({
                "id": id,
                "object": "model",
                "created": created_at,
                "owned_by": "image_core",
                "name": name,
                "kind": kind,
            })
        })
        .collect();
    json_response(StatusCode::OK, json!({"object":"list","data": data}))
}

// ----------------------------- Edits -----------------------------
//
// POST /v1/images/edits  (multipart/form-data)
// Required: image (one or more), prompt, model
// Optional: n, size, response_format, seed
//
// POST /v1/images/variations  (multipart/form-data)
// Required: image, model
// Optional: n, size, response_format

pub async fn create_image_edit(
    State(state): State<AppState>,
    Extension(auth): Extension<crate::apikey_auth::AuthedKey>,
    mut form: Multipart,
) -> Response {
    handle_image_edit_or_variation(&state, &auth, &mut form, true).await
}

pub async fn create_image_variation(
    State(state): State<AppState>,
    Extension(auth): Extension<crate::apikey_auth::AuthedKey>,
    mut form: Multipart,
) -> Response {
    handle_image_edit_or_variation(&state, &auth, &mut form, false).await
}

/// Shared handler for edits (requires prompt) and variations (prompt optional).
async fn handle_image_edit_or_variation(
    state: &AppState,
    auth: &crate::apikey_auth::AuthedKey,
    form: &mut Multipart,
    require_prompt: bool,
) -> Response {
    // Parse form
    let mut model_id: Option<String> = None;
    let mut prompt: Option<String> = None;
    let mut size: Option<String> = None;
    let mut response_format: Option<String> = None;
    let mut seed: Option<i64> = None;
    let mut n: Option<u32> = None;
    let mut images: Vec<(String, Vec<u8>)> = Vec::new(); // (filename, bytes)

    loop {
        let field = match form.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return json_response(
                    StatusCode::BAD_REQUEST,
                    json!({"error":{"code":"invalid_multipart","message":format!("{e}")}}),
                );
            }
        };
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "model" => {
                model_id = Some(field.text().await.unwrap_or_default());
            }
            "prompt" => {
                prompt = Some(field.text().await.unwrap_or_default());
            }
            "n" => {
                if let Ok(t) = field.text().await {
                    n = t.parse().ok();
                }
            }
            "size" => {
                size = Some(field.text().await.unwrap_or_default());
            }
            "response_format" => {
                response_format = Some(field.text().await.unwrap_or_default());
            }
            "seed" => {
                if let Ok(t) = field.text().await {
                    seed = t.parse().ok();
                }
            }
            "image" | "image[]" => {
                let filename = field
                    .file_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("upload-{}.png", uuid::Uuid::new_v4()));
                let bytes = match field.bytes().await {
                    Ok(b) => b.to_vec(),
                    Err(e) => {
                        return json_response(
                            StatusCode::BAD_REQUEST,
                            json!({"error":{"code":"invalid_image","message":format!("{e}")}}),
                        );
                    }
                };
                images.push((filename, bytes));
            }
            _ => {
                // ignore unknown fields
            }
        }
    }

    let model_id = match model_id {
        Some(m) if !m.is_empty() => m,
        _ => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({"error":{"code":"missing_model","message":"model field required"}}),
            );
        }
    };
    if images.is_empty() {
        return json_response(
            StatusCode::BAD_REQUEST,
            json!({"error":{"code":"missing_image","message":"at least one image required"}}),
        );
    }
    if require_prompt && prompt.as_deref().unwrap_or("").is_empty() {
        return json_response(
            StatusCode::BAD_REQUEST,
            json!({"error":{"code":"missing_prompt","message":"prompt field required"}}),
        );
    }

    // Resolve size
    let (width, height) = match parse_size(size.as_deref()) {
        Ok(s) => s,
        Err(e) => return json_response(StatusCode::BAD_REQUEST, json!({"error":{"code":"invalid_size","message":e}})),
    };
    let response_format = response_format.as_deref().unwrap_or("url");
    let n = n.unwrap_or(1).max(1);

    // Find model
    let model = match find_enabled_model(state, &model_id).await {
        Ok(m) => m,
        Err(resp) => return resp,
    };
    if model.mapping.image.is_empty() {
        return json_response(
            StatusCode::BAD_REQUEST,
            json!({"error":{"code":"no_image_input","message":"model does not accept image input"}}),
        );
    }

    let timer = crate::logging::timer();
    let mut draft = LogDraft::new("openai");
    let request_body_marker = format!(
        "{{\"model\":\"{model_id}\",\"prompt\":\"{}\",\"n\":{},\"size\":\"{}\"}}",
        prompt.clone().unwrap_or_default(),
        n,
        size.clone().unwrap_or_default(),
    );
    draft.request_body = Some(Value::String(request_body_marker));

    let mut params = GenParams::default();
    params.prompt = prompt.clone().unwrap_or_default();
    params.width = Some(width);
    params.height = Some(height);
    params.seed = seed;
    params.batch_size = Some(n);

    // Upload each user image to ComfyUI and remember the (first) saved name.
    let mut uploaded_names: Vec<String> = Vec::new();
    for (filename, bytes) in &images {
        match state.comfy.upload_image(bytes.clone(), filename, true).await {
            Ok(n) => uploaded_names.push(n),
            Err(e) => {
                let msg = format!("upload failed: {e}");
                let draft = apply_draft_basics(
                    draft,
                    &model_id,
                    prompt.as_deref(),
                    Some(format!("{width}x{height}")),
                    seed,
                    None,
                    Some(auth.name.clone()),
                    timer.elapsed_ms(),
                    Vec::new(),
                    500,
                    Some(msg.clone()),
                );
                draft.commit(state).await;
                return json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({"error":{"code":"upload_failed","message":msg}}),
                );
            }
        }
    }

    // Build the workflow, then patch the image input with the first uploaded
    // name. ComfyUI treats LoadImage.image as a string filename.
    let (mut workflow, final_seed) =
        state
            .comfy
            .build_workflow(&model.workflow, &model.mapping, &params);
    let chosen = uploaded_names.first().cloned().unwrap_or_default();
    crate::mapping::write_field(&mut workflow, &model.mapping.image, Value::String(chosen));

    let timeout_s = state.cfg.request_timeout_s;
    let result = state
        .comfy
        .run_queued(
            &state.queue,
            &model.name,
            params.prompt.as_str(),
            &workflow,
            final_seed,
            timeout_s,
        )
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
            let data = build_data_items(&urls, response_format, state);
            let body = json!({
                "created": chrono::Utc::now().timestamp(),
                "data": data,
            });
            let draft = apply_draft_basics(
                draft,
                &model_id,
                prompt.as_deref(),
                Some(format!("{width}x{height}")),
                Some(final_seed),
                None,
                Some(auth.name.clone()),
                elapsed,
                urls,
                200,
                None,
            );
            draft.commit(state).await;
            json_response(StatusCode::OK, body)
        }
        Err(e) => {
            let msg = e.to_string();
            let draft = apply_draft_basics(
                draft,
                &model_id,
                prompt.as_deref(),
                Some(format!("{width}x{height}")),
                seed,
                None,
                Some(auth.name.clone()),
                elapsed,
                Vec::new(),
                500,
                Some(msg.clone()),
            );
            draft.commit(state).await;
            json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error":{"code":"generation_failed","message":msg}}),
            )
        }
    }
}