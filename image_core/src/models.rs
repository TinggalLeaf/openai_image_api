//! Core domain types shared across modules.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// A model record: workflow JSON + parameter mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub kind: String,
    pub workflow: Value,
    pub mapping: ParamMapping,
    #[serde(rename = "created_at")]
    pub created_at: i64,
    #[serde(rename = "updated_at")]
    pub updated_at: i64,
}

/// Point-separated path map: key → "nodeId.inputs.field".
/// Empty string means "not mapped" (use workflow default).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParamMapping {
    #[serde(default, rename = "prompt")]
    pub prompt: String,
    #[serde(default, rename = "negative_prompt")]
    pub negative_prompt: String,
    #[serde(default, rename = "width")]
    pub width: String,
    #[serde(default, rename = "height")]
    pub height: String,
    #[serde(default, rename = "seed")]
    pub seed: String,
    #[serde(default, rename = "steps")]
    pub steps: String,
    #[serde(default, rename = "cfg")]
    pub cfg: String,
    #[serde(default, rename = "sampler")]
    pub sampler: String,
    #[serde(default, rename = "scheduler")]
    pub scheduler: String,
    #[serde(default, rename = "batch_size")]
    pub batch_size: String,
    /// Path to a LoadImage `image` input — used by /v1/images/edits to wire
    /// the user-uploaded image into the workflow.
    #[serde(default, rename = "image")]
    pub image: String,
}

impl ParamMapping {
    pub fn as_hashmap(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("prompt".into(), self.prompt.clone());
        m.insert("negative_prompt".into(), self.negative_prompt.clone());
        m.insert("width".into(), self.width.clone());
        m.insert("height".into(), self.height.clone());
        m.insert("seed".into(), self.seed.clone());
        m.insert("steps".into(), self.steps.clone());
        m.insert("cfg".into(), self.cfg.clone());
        m.insert("sampler".into(), self.sampler.clone());
        m.insert("scheduler".into(), self.scheduler.clone());
        m.insert("batch_size".into(), self.batch_size.clone());
        m
    }
}

/// Normalized generation parameters — the internal canonical representation.
#[derive(Debug, Clone, Default)]
pub struct GenParams {
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub cfg: Option<f64>,
    pub seed: Option<i64>,
    pub sampler: Option<String>,
    pub scheduler: Option<String>,
    pub batch_size: Option<u32>,
}

/// Result of a generation: list of public image URLs and final seed used.
#[derive(Debug, Clone)]
pub struct GenResult {
    pub image_urls: Vec<String>,
    pub seed_used: i64,
}

/// API key record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub name: String,
    pub key: String,
    pub enabled: bool,
    #[serde(rename = "created_at")]
    pub created_at: i64,
}

/// Log entry returned by the admin API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub ts: i64,
    pub provider: String,
    pub key_name: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub size: Option<String>,
    pub seed: Option<i64>,
    pub status: String,
    pub error: Option<String>,
    pub latency_ms: i64,
    pub image_urls: Option<String>,
    pub request_body: Option<String>,
    pub response_status: Option<i64>,
}

/// Editable settings persisted in DB. Values missing from the table fall
/// back to the live `Config` (which already includes .env + env defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub comfyui_url: String,
    pub request_timeout_s: u64,
    pub listen_addr: String,
    pub data_dir: String,
    pub max_concurrent: u32,
    pub image_retention_hours: u32,
    pub image_max_total_mb: u32,
}