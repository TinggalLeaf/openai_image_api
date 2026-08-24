//! ComfyUI execution: clone workflow, write params, POST, listen WS for done, fetch images.

use crate::config::Config;
use crate::mapping::{read_field, write_field};
use crate::models::{GenParams, ParamMapping};
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use rand::Rng;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

#[derive(Clone)]
pub struct ComfyExecutor {
    pub http: reqwest::Client,
    pub cfg: Config,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("comfyui unreachable: {0}")]
    Unreachable(String),
    #[error("submit failed: {0}")]
    Submit(String),
    #[error("execution error: {0}")]
    Execution(String),
    #[error("timeout after {0}s")]
    Timeout(u64),
    #[error("no images in output")]
    NoImages,
    #[error("http error: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl ComfyExecutor {
    pub fn new(cfg: Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("build reqwest client");
        Self { http, cfg }
    }

    pub async fn comfyui_online(&self) -> bool {
        // /system_stats is a lightweight ping endpoint on ComfyUI.
        match self
            .http
            .get(format!("{}/system_stats", self.cfg.comfyui_url))
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    /// Acquire a queue permit, then run a workflow. The permit guard lives for
    /// the entire execution (queue + ComfyUI run + image fetch) so that
    /// `max_concurrent` is respected end-to-end.
    pub async fn run_queued(
        &self,
        queue: &crate::queue::QueueManager,
        model_name: &str,
        prompt_preview: &str,
        workflow: &Value,
        seed_used: i64,
        timeout_s: u64,
    ) -> Result<Vec<String>, ExecError> {
        let _guard = queue
            .acquire(model_name, prompt_preview)
            .await
            .map_err(|e| ExecError::Submit(format!("queue acquire: {e}")))?;
        self.execute(workflow, seed_used, timeout_s).await
    }

    /// Upload a file to ComfyUI's /upload/image endpoint and return the
    /// stored filename (which the caller can then write into a LoadImage
    /// `image` input).
    pub async fn upload_image(
        &self,
        bytes: Vec<u8>,
        original_filename: &str,
        overwrite: bool,
    ) -> Result<String, ExecError> {
        let url = format!("{}/upload/image", self.cfg.comfyui_url);
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(original_filename.to_string());
        let form = reqwest::multipart::Form::new()
            .text("type", "input")
            .text("overwrite", if overwrite { "true" } else { "false" })
            .part("image", part);
        let resp = self
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| ExecError::Http(format!("upload: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ExecError::Http(format!(
                "upload failed: status {}: {}",
                status, text
            )));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| ExecError::Http(format!("upload json: {e}")))?;
        let name = v
            .get("name")
            .and_then(|x| x.as_str())
            .ok_or_else(|| ExecError::Http("upload: missing name".into()))?;
        Ok(name.to_string())
    }

    /// Build the workflow JSON with normalized params written in via mapping.
    pub fn build_workflow(
        &self,
        template: &Value,
        mapping: &ParamMapping,
        params: &GenParams,
    ) -> (Value, i64) {
        let mut wf = template.clone();
        let mut rng = rand::thread_rng();
        let final_seed: i64 = match params.seed {
            Some(s) if s >= 0 => s,
            _ => rng.gen_range(1..=i64::MAX),
        };

        let fields: Vec<(&str, &str, Value)> = vec![
            (
                &mapping.prompt,
                "prompt",
                Value::String(params.prompt.clone()),
            ),
            (
                &mapping.negative_prompt,
                "negative_prompt",
                params
                    .negative_prompt
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
            (
                &mapping.width,
                "width",
                params.width.map(|v| json!(v)).unwrap_or(Value::Null),
            ),
            (
                &mapping.height,
                "height",
                params.height.map(|v| json!(v)).unwrap_or(Value::Null),
            ),
            (
                &mapping.seed,
                "seed",
                json!(final_seed),
            ),
            (
                &mapping.steps,
                "steps",
                params.steps.map(|v| json!(v)).unwrap_or(Value::Null),
            ),
            (
                &mapping.cfg,
                "cfg",
                params.cfg.map(|v| json!(v)).unwrap_or(Value::Null),
            ),
            (
                &mapping.sampler,
                "sampler",
                params
                    .sampler
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
            (
                &mapping.scheduler,
                "scheduler",
                params
                    .scheduler
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            ),
            (
                &mapping.batch_size,
                "batch_size",
                params.batch_size.map(|v| json!(v)).unwrap_or(Value::Null),
            ),
        ];

        for (path, _name, val) in fields {
            if path.is_empty() {
                continue;
            }
            if val.is_null() {
                continue;
            }
            // Skip if the field is currently a link array (comfyui will reject writes there).
            if let Some(existing) = read_field(&wf, path) {
                if existing.is_array() {
                    continue;
                }
            }
            write_field(&mut wf, path, val);
        }

        (wf, final_seed)
    }

    /// Submit a workflow and wait for completion. Returns list of saved file paths
    /// (absolute). The caller is responsible for passing in `seed_used` so it can
    /// be returned to the API client / persisted in the log row.
    pub async fn execute(
        &self,
        workflow: &Value,
        seed_used: i64,
        timeout_s: u64,
    ) -> Result<Vec<String>, ExecError> {
        let client_id = uuid::Uuid::new_v4().to_string();
        let prompt_id = self
            .submit(workflow, &client_id)
            .await
            .map_err(|e| ExecError::Submit(e.to_string()))?;

        let timeout_dur = Duration::from_secs(timeout_s.max(5));
        match timeout(
            timeout_dur,
            self.wait_for_completion_ws(&client_id, &prompt_id),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(_)) | Err(_) => {
                // Either WS errored or we timed out — fall back to polling.
                self.poll_until_done(&prompt_id, timeout_s)
                    .await
                    .map_err(|e| match e {
                        ExecError::Timeout(_) => e,
                        other => other,
                    })?;
            }
        }

        let images = self
            .fetch_and_save_images(&prompt_id)
            .await
            .map_err(|e| match e {
                ExecError::NoImages => ExecError::NoImages,
                other => other,
            })?;

        if images.is_empty() {
            return Err(ExecError::NoImages);
        }
        let _ = seed_used; // accepted for API symmetry with callers
        Ok(images)
    }

    async fn submit(&self, workflow: &Value, client_id: &str) -> Result<String> {
        let url = format!("{}/prompt", self.cfg.comfyui_url);
        let body = json!({ "prompt": workflow, "client_id": client_id });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("POST /prompt")?;
        let status = resp.status();
        let text = resp.text().await.context("read prompt response")?;
        if !status.is_success() {
            return Err(anyhow!("status {}: {}", status, text));
        }
        let v: Value = serde_json::from_str(&text).context("parse prompt response")?;
        v.get("prompt_id")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("no prompt_id in response: {}", text))
    }

    async fn wait_for_completion_ws(
        &self,
        client_id: &str,
        expected_prompt_id: &str,
    ) -> Result<()> {
        let ws_url = format!(
            "{}/ws?clientId={}",
            self.cfg.comfyui_ws_url(),
            client_id
        );
        let req = ws_url.into_client_request().context("build ws request")?;

        let (mut ws, _resp) = tokio_tungstenite::connect_async(req)
            .await
            .context("ws connect")?;
        tracing::info!(client_id, expected_prompt_id, "ws connected");

        while let Some(msg) = ws.next().await {
            let msg = msg.context("ws message")?;
            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    let kind = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
                    let data = v.get("data");
                    let pid = data
                        .and_then(|d| d.get("prompt_id"))
                        .and_then(|p| p.as_str())
                        .unwrap_or("");
                    if pid != expected_prompt_id && kind != "status" {
                        continue;
                    }
                    match kind {
                        "execution_success" => return Ok(()),
                        "execution_error" => {
                            let msg = data
                                .and_then(|d| d.get("exception_message"))
                                .and_then(|e| e.as_str())
                                .unwrap_or("unknown error");
                            return Err(anyhow!("comfyui execution error: {}", msg));
                        }
                        "executing" => {
                            // data.node == null and prompt_id matches → idle/done
                            let node = data.and_then(|d| d.get("node"));
                            if node.map(|n| n.is_null()).unwrap_or(false) {
                                return Ok(());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Err(anyhow!("ws closed before completion"))
    }

    async fn poll_until_done(&self, prompt_id: &str, timeout_s: u64) -> Result<(), ExecError> {
        let url = format!("{}/history/{}", self.cfg.comfyui_url, prompt_id);
        let deadline = Instant::now() + Duration::from_secs(timeout_s.max(5));
        loop {
            if Instant::now() >= deadline {
                return Err(ExecError::Timeout(timeout_s));
            }
            if let Ok(resp) = self.http.get(&url).send().await {
                if let Ok(v) = resp.json::<Value>().await {
                    if let Some(entry) = v.get(prompt_id) {
                        let status = entry.get("status");
                        // ComfyUI history entry has "status": { "completed": bool, "messages": [...] }
                        // and outputs at .outputs.{node_id}.images
                        if let Some(s) = status {
                            if s.get("completed").and_then(|x| x.as_bool()).unwrap_or(false) {
                                return Ok(());
                            }
                        }
                        // Fallback: outputs present implies done
                        if entry.get("outputs").is_some() {
                            return Ok(());
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn fetch_and_save_images(&self, prompt_id: &str) -> Result<Vec<String>, ExecError> {
        let url = format!("{}/history/{}", self.cfg.comfyui_url, prompt_id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| ExecError::Http(e.to_string()))?;
        let body: Value = resp
            .json()
            .await
            .map_err(|e| ExecError::Http(e.to_string()))?;

        let entry = body
            .get(prompt_id)
            .ok_or_else(|| ExecError::Execution("prompt_id missing in history".into()))?;
        let outputs = entry.get("outputs");

        // Collect image descriptors: Vec<(filename, subfolder, type)>
        let mut found: Vec<(String, String, String)> = Vec::new();
        if let Some(outputs) = outputs.and_then(|o| o.as_object()) {
            for (_nid, node) in outputs {
                if let Some(imgs) = node.get("images").and_then(|i| i.as_array()) {
                    for img in imgs {
                        let filename = img
                            .get("filename")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let subfolder = img
                            .get("subfolder")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let ty = img
                            .get("type")
                            .and_then(|x| x.as_str())
                            .unwrap_or("output")
                            .to_string();
                        if !filename.is_empty() {
                            found.push((filename, subfolder, ty));
                        }
                    }
                }
            }
        }
        if found.is_empty() {
            return Err(ExecError::NoImages);
        }

        tokio::fs::create_dir_all(&self.cfg.images_dir)
            .await
            .map_err(ExecError::Io)?;
        let mut saved: Vec<String> = Vec::new();
        for (filename, subfolder, ty) in found {
            let url = format!(
                "{}/view?filename={}&subfolder={}&type={}",
                self.cfg.comfyui_url,
                urlencode(&filename),
                urlencode(&subfolder),
                urlencode(&ty)
            );
            let bytes = self
                .http
                .get(&url)
                .send()
                .await
                .and_then(|r| r.error_for_status())
                .map_err(|e| ExecError::Http(e.to_string()))?
                .bytes()
                .await
                .map_err(|e| ExecError::Http(e.to_string()))?;
            let ext = Path::new(&filename)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("png");
            let new_name = format!("{}.{}", uuid::Uuid::new_v4(), ext);
            let abs = Path::new(&self.cfg.images_dir).join(&new_name);
            tokio::fs::write(&abs, &bytes)
                .await
                .map_err(ExecError::Io)?;
            saved.push(abs.to_string_lossy().to_string());
        }
        Ok(saved)
    }
}

fn urlencode(s: &str) -> String {
    // Minimal percent-encoding for query values: keep alnum and -_.~
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}