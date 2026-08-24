//! Volcengine Ark Seedream-compatible image generation.

use crate::logging::LogDraft;
use crate::models::GenParams;
use crate::routes::common::{
    apply_draft_basics, bad_request_error, file_to_base64, find_enabled_model, json_response,
    parse_size, run_model,
};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Extension;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct VolcengineReq {
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub response_format: Option<String>,
    #[serde(default)]
    pub watermark: Option<bool>,
    #[serde(default)]
    pub seed: Option<i64>,
    #[serde(default)]
    pub guidance_scale: Option<f64>,
}

pub async fn create_image(
    State(state): State<AppState>,
    Extension(auth): Extension<crate::apikey_auth::AuthedKey>,
    axum::Json(req): axum::Json<Value>,
) -> Response {
    let parsed: VolcengineReq = match serde_json::from_value(req.clone()) {
        Ok(p) => p,
        Err(e) => return bad_request_error("invalid_request", &format!("{e}")),
    };

    let response_format = parsed.response_format.as_deref().unwrap_or("url");

    // Volcengine sizes: "WxH" | "adaptive" | "1K" | "2K" | "4K"
    let (width, height) = match parse_size(parsed.size.as_deref()) {
        Ok(s) => s,
        Err(e) => return bad_request_error("invalid_size", &e),
    };

    let model = match find_enabled_model(&state, &parsed.model).await {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let timer = crate::logging::timer();
    let mut draft = LogDraft::new("volcengine");
    draft.request_body = Some(req.clone());

    let mut params = GenParams::default();
    params.prompt = parsed.prompt.clone();
    params.width = Some(width);
    params.height = Some(height);
    params.seed = parsed.seed;
    params.cfg = parsed.guidance_scale;
    params.batch_size = Some(1);

    let result = run_model(&state, &state.comfy, &model, &params).await;
    let elapsed = timer.elapsed_ms();
    match result {
        Ok((urls, seed_used)) => {
            let data: Vec<Value> = urls
                .iter()
                .map(|u| {
                    let fname = u.trim_start_matches("/files/images/");
                    let abs = std::path::Path::new(&state.cfg.images_dir).join(fname);
                    if response_format == "b64_json" {
                        json!({"b64_json": file_to_base64(abs.to_str().unwrap_or("")).unwrap_or_default()})
                    } else {
                        json!({"url": u})
                    }
                })
                .collect();
            let body = json!({
                "model": parsed.model,
                "created": chrono::Utc::now().timestamp(),
                "data": data,
                "usage": {"generated_images": urls.len()},
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