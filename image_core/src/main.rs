//! image_core — ComfyUI ↔ multi-provider image generation gateway.

mod apikey_auth;
mod auth;
mod cleanup;
mod comfyui;
mod config;
mod db;
mod embedded;
mod logging;
mod mapping;
mod models;
mod queue;
mod routes;
mod state;
mod workflow_convert;

use crate::comfyui::ComfyExecutor;
use crate::config::Config;
use crate::queue::QueueManager;
use crate::state::AppState;
use std::net::SocketAddr;
use std::path::Path;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    // Load .env before Config::from_env so its values are visible.
    load_dotenv_with_template();

    let cfg = Config::from_env();
    tracing::info!(
        listen = %cfg.listen_addr,
        comfyui = %cfg.comfyui_url,
        data_dir = %cfg.data_dir,
        admin_auth = cfg.admin_auth_enabled(),
        "starting image_core"
    );

    tokio::fs::create_dir_all(&cfg.data_dir).await.ok();
    tokio::fs::create_dir_all(&cfg.images_dir).await.ok();

    let pool = db::init_pool(&cfg).await?;
    let comfy = ComfyExecutor::new(cfg.clone());
    let queue = QueueManager::new(cfg.max_concurrent);
    let state = AppState {
        pool: pool.clone(),
        cfg: cfg.clone(),
        comfy,
        queue: queue.clone(),
        admin_tokens: auth::new_token_store(),
    };

    // Background: periodic image cleanup.
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        cleanup::run_cleanup_loop(cleanup_state).await;
    });

    let app = routes::build_router(state);

    let addr: SocketAddr = cfg.listen_addr.parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

/// Locate an existing `.env` and load it via dotenvy. If none is found,
/// write the template to the executable's directory (or cwd if that's
/// where we are) so first-launch users have a starting point.
fn load_dotenv_with_template() {
    // dotenvy by default probes ".env" in the cwd. Force-load from the
    // location we discovered, if any.
    let path = config::locate_env_file();
    match path {
        Some(p) => {
            // dotenvy::from_path leaves already-set process-env vars alone,
            // so the precedence is "baked-in defaults → .env → process env".
            // Use the variant that does NOT override.
            let _ = dotenvy::from_path(&p);
            tracing::info!(env = %p.display(), "loaded .env");
        }
        None => {
            // No .env found — write the template to the most useful spot:
            // exe-relative when possible (single-file release), otherwise cwd.
            let target = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join(".env")))
                .unwrap_or_else(|| std::path::PathBuf::from(".env"));
            match config::write_env_template_if_missing(&target) {
                Ok(p) => tracing::info!(path = %p.display(), "no .env found — wrote starter template"),
                Err(e) => tracing::warn!("could not write .env template: {e}"),
            }
        }
    }
    // Make sure DATA_DIR's parent exists so db/cleanup paths work even
    // before any handler hits them.
    if let Some(dir) = Path::new(
        &std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into()),
    )
    .parent()
    {
        if !dir.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}