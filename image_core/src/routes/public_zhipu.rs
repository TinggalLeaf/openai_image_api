//! Zhipu CogView-compatible image generation.

use crate::logging::LogDraft;
use crate::models::GenParams;
use crate::routes::common::{
    apply_draft_basics, bad_request_error, find_enabled_model, json_response, parse_size,
    run_model,
};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::Extension;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
pub struct ZhipuReq {
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

pub async fn create_image(
    State(state): State<AppState>,
    Extension(auth): Extension<crate::apikey_auth::AuthedKey>,
    axum::Json(req): axum::Json<Value>,
) -> Response {
    let parsed: ZhipuReq = match serde_json::from_value(req.clone()) {
        Ok(p) => p,
        Err(e) => return bad_request_error("invalid_request", &format!("{e}")),
    };

    let (width, height) = match parse_size(parsed.size.as_deref()) {
        Ok(s) => s,
        Err(e) => return bad_request_error("invalid_size", &e),
    };

    let model = match find_enabled_model(&state, &parsed.model).await {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let timer = crate::logging::timer();
    let mut draft = LogDraft::new("zhipu");
    draft.request_body = Some(req.clone());

    let mut params = GenParams::default();
    params.prompt = parsed.prompt.clone();
    params.width = Some(width);
    params.height = Some(height);
    params.batch_size = Some(1);

    let result = run_model(&state, &state.comfy, &model, &params).await;
    let elapsed = timer.elapsed_ms();
    match result {
        Ok((urls, seed_used)) => {
            let data: Vec<Value> = urls.iter().map(|u| json!({"url": u})).collect();
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
                None,
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