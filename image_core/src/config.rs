//! Runtime configuration loaded from .env / env vars / defaults.

use std::env;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub listen_addr: String,
    pub comfyui_url: String,
    pub request_timeout_s: u64,
    pub data_dir: String,
    pub db_path: String,
    pub images_dir: String,
    pub max_concurrent: u32,
    pub image_retention_hours: u32,
    pub image_max_total_mb: u32,
    /// Plain-text admin password. Empty string means admin auth is disabled.
    pub admin_password: String,
}

impl Config {
    /// Load configuration. Order:
    /// 1. Defaults baked into this binary.
    /// 2. `.env` file in the executable's directory or the repo root (whichever
    ///    is found first). Loaded via `dotenvy`.
    /// 3. Process environment (already populated by step 2).
    pub fn from_env() -> Self {
        let listen_addr =
            env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8000".to_string());
        let comfyui_url = env::var("COMFYUI_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8188".to_string());
        let request_timeout_s = env::var("REQUEST_TIMEOUT_S")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300u64);
        let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
        let max_concurrent = env::var("MAX_CONCURRENT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2u32);
        let image_retention_hours = env::var("IMAGE_RETENTION_HOURS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(72u32);
        let image_max_total_mb = env::var("IMAGE_MAX_TOTAL_MB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024u32);
        let admin_password = env::var("ADMIN_PASSWORD").unwrap_or_default();

        let data_dir_path = Path::new(&data_dir);
        let db_path = join_path(data_dir_path, "image_core.db");
        let images_dir = join_path(data_dir_path, "images");

        Self {
            listen_addr,
            comfyui_url,
            request_timeout_s,
            data_dir,
            db_path,
            images_dir,
            max_concurrent,
            image_retention_hours,
            image_max_total_mb,
            admin_password,
        }
    }

    pub fn comfyui_ws_url(&self) -> String {
        if let Some(rest) = self.comfyui_url.strip_prefix("http://") {
            format!("ws://{}", rest)
        } else if let Some(rest) = self.comfyui_url.strip_prefix("https://") {
            format!("wss://{}", rest)
        } else {
            self.comfyui_url.clone()
        }
    }

    pub fn admin_auth_enabled(&self) -> bool {
        !self.admin_password.is_empty()
    }
}

fn join_path(base: &Path, tail: &str) -> String {
    let joined = base.join(tail);
    // Normalize to forward slashes for consistency.
    joined.to_string_lossy().replace('\\', "/")
}

/// Find a `.env` to load, in this order:
///   1. `executable_dir/.env` (single-file release layout)
///   2. `cwd/.env`
///   3. `cwd/../.env` (one level up — useful when cwd is `image_core/` during
///      a dev build and the user keeps the env at the repo root)
///   4. `cwd/../../.env` (two levels up)
/// Returns the path of the first .env found, or None.
pub fn locate_env_file() -> Option<PathBuf> {
    fn from_exe() -> Option<PathBuf> {
        let exe = env::current_exe().ok()?;
        exe.parent().map(|d| d.join(".env"))
    }
    fn from_cwd() -> Option<PathBuf> {
        env::current_dir().ok().map(|d| d.join(".env"))
    }
    fn from_manifest() -> Option<PathBuf> {
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(".env"))
    }
    let candidates: [Option<PathBuf>; 5] = [
        from_exe(),
        from_cwd(),
        env::current_dir().ok().map(|d| d.join("..").join(".env")),
        env::current_dir().ok().map(|d| d.join("..").join("..").join(".env")),
        from_manifest(),
    ];
    for c in candidates.into_iter().flatten() {
        if c.is_file() {
            return Some(c);
        }
    }
    None
}

/// Write a starter `.env` template at `target` if no `.env` already exists
/// in the same directory. Returns the path that ended up with the file.
pub fn write_env_template_if_missing(target: &Path) -> std::io::Result<PathBuf> {
    if target.exists() {
        return Ok(target.to_path_buf());
    }
    let body = ENV_TEMPLATE;
    std::fs::write(target, body)?;
    Ok(target.to_path_buf())
}

/// Canonical template that gets written when no `.env` is present.
pub const ENV_TEMPLATE: &str = r#"# image_core — local config
# 删除这一行就回到内置默认值。任何键都可以不写。
#
# 监听地址（绑定 0.0.0.0:8000 = 接受所有接口）。改成 127.0.0.1:8000 仅本地访问。
LISTEN_ADDR=0.0.0.0:8000

# 上游 ComfyUI 地址。
COMFYUI_URL=http://127.0.0.1:8188

# 单次请求超时（秒）。包含排队 + ComfyUI 执行 + 图片拉取。
REQUEST_TIMEOUT_S=300

# 数据目录。SQLite 库与生成的图片都放在这里。
DATA_DIR=./data

# ComfyUI 并发上限（同时执行的请求数）。
MAX_CONCURRENT=2

# 图片自动清理：超过这个小时数的文件会被删除；0 = 永远保留。
IMAGE_RETENTION_HOURS=72

# 图片目录总量上限（MB）。超过后按 mtime 从旧到新淘汰；0 = 不限。
IMAGE_MAX_TOTAL_MB=1024

# 管理面板密码。留空 = 不启用鉴权（公开访问 /admin/api/*）。
# 设置后访问 /admin/api/* 需要 Authorization: Bearer <token>；
# 通过 POST /admin/api/login {"password":"..."} 拿 token。
ADMIN_PASSWORD=
"#;