//! Request logging helper — write a row to `logs` after each public/admin generation.

use crate::models::LogEntry;
use crate::state::AppState;
use serde_json::Value;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct LogDraft {
    pub provider: String,
    pub key_name: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub size: Option<String>,
    pub seed: Option<i64>,
    pub status: String,
    pub error: Option<String>,
    pub latency_ms: i64,
    pub image_urls: Vec<String>,
    pub request_body: Option<Value>,
    pub response_status: Option<i64>,
}

impl LogDraft {
    pub fn new(provider: &str) -> Self {
        Self {
            provider: provider.into(),
            key_name: None,
            model: None,
            prompt: None,
            size: None,
            seed: None,
            status: "success".into(),
            error: None,
            latency_ms: 0,
            image_urls: Vec::new(),
            request_body: None,
            response_status: None,
        }
    }

    pub fn error(mut self, e: impl ToString) -> Self {
        self.status = "error".into();
        self.error = Some(e.to_string());
        self
    }

    /// Persist to DB. request_body is truncated to 4 KB.
    pub async fn commit(self, state: &AppState) {
        let req_body = self
            .request_body
            .as_ref()
            .map(|v| v.to_string())
            .map(|s| truncate(&s, 4096));
        let urls_json = serde_json::to_string(&self.image_urls).unwrap_or_else(|_| "[]".into());
        let id = uuid::Uuid::new_v4().to_string();
        let ts = chrono::Utc::now().timestamp();

        let res = sqlx::query(
            "INSERT INTO logs (id, ts, provider, key_name, model, prompt, size, seed, status, error, latency_ms, image_urls, request_body, response_status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(ts)
        .bind(&self.provider)
        .bind(&self.key_name)
        .bind(&self.model)
        .bind(self.prompt.as_deref().map(truncate_512))
        .bind(&self.size)
        .bind(self.seed)
        .bind(&self.status)
        .bind(&self.error)
        .bind(self.latency_ms)
        .bind(&urls_json)
        .bind(&req_body)
        .bind(self.response_status)
        .execute(&state.pool)
        .await;

        if let Err(e) = res {
            tracing::warn!("log insert failed: {e}");
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Truncate at char boundary
        let mut idx = max;
        while !s.is_char_boundary(idx) && idx > 0 {
            idx -= 1;
        }
        format!("{}…", &s[..idx])
    }
}

fn truncate_512(s: &str) -> String {
    truncate(s, 512)
}

/// Convenience: time the closure and build a log draft.
pub fn timer() -> Timer {
    Timer { start: Instant::now() }
}

pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn elapsed_ms(&self) -> i64 {
        self.start.elapsed().as_millis() as i64
    }
}

#[allow(dead_code)]
pub fn log_to_entry(id: String, draft: LogDraft) -> LogEntry {
    LogEntry {
        id,
        ts: chrono::Utc::now().timestamp(),
        provider: draft.provider,
        key_name: draft.key_name,
        model: draft.model,
        prompt: draft.prompt,
        size: draft.size,
        seed: draft.seed,
        status: draft.status,
        error: draft.error,
        latency_ms: draft.latency_ms,
        image_urls: Some(serde_json::to_string(&draft.image_urls).unwrap_or_else(|_| "[]".into())),
        request_body: draft.request_body.map(|v| v.to_string()),
        response_status: draft.response_status,
    }
}