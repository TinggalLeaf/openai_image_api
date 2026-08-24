//! Shared application state passed to every handler.

use crate::auth::TokenStore;
use crate::comfyui::ComfyExecutor;
use crate::config::Config;
use crate::queue::QueueManager;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub cfg: Config,
    pub comfy: ComfyExecutor,
    pub queue: QueueManager,
    pub admin_tokens: TokenStore,
}