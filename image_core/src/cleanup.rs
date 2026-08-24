//! Image cleanup: enforce `image_retention_hours` (mtime-based) and
//! `image_max_total_mb` (size-based).
//!
//! Runs every 10 minutes in the background and is also exposed as the manual
//! `POST /admin/api/storage/cleanup` endpoint.

use crate::state::AppState;
use serde::Serialize;
use std::time::Duration;
use tokio::time::sleep;

const SCAN_INTERVAL_S: u64 = 600; // 10 min

#[derive(Debug, Clone, Serialize)]
pub struct StorageStats {
    pub dir: String,
    pub images_count: u64,
    pub total_bytes: u64,
    pub retention_hours: u32,
    pub max_total_mb: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupResult {
    pub deleted_count: u64,
    pub freed_bytes: u64,
}

/// Collect all files in the images dir along with size + mtime.
async fn scan_dir(dir: &str) -> Vec<(String, u64, i64)> {
    let mut out = Vec::new();
    let mut rd = match tokio::fs::read_dir(dir).await {
        Ok(r) => r,
        Err(_) => return out,
    };
    while let Ok(Some(entry)) = rd.next_entry().await {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let meta = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        out.push((path.to_string_lossy().to_string(), size, mtime));
    }
    out
}

/// Compute the current retention-hours and max_total_mb values from settings,
/// falling back to config defaults.
async fn read_thresholds(state: &AppState) -> (u32, u32) {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
    let mut ret = state.cfg.image_retention_hours;
    let mut mb = state.cfg.image_max_total_mb;
    for (k, v) in rows {
        match k.as_str() {
            "image_retention_hours" => {
                if let Ok(n) = v.parse::<u32>() {
                    ret = n;
                }
            }
            "image_max_total_mb" => {
                if let Ok(n) = v.parse::<u32>() {
                    mb = n;
                }
            }
            _ => {}
        }
    }
    (ret, mb)
}

pub async fn storage_stats(state: &AppState) -> StorageStats {
    let (retention_hours, max_total_mb) = read_thresholds(state).await;
    let files = scan_dir(&state.cfg.images_dir).await;
    let total_bytes = files.iter().map(|(_, s, _)| *s).sum();
    StorageStats {
        dir: state.cfg.images_dir.clone(),
        images_count: files.len() as u64,
        total_bytes,
        retention_hours,
        max_total_mb,
    }
}

pub async fn run_cleanup(state: &AppState) -> CleanupResult {
    let (retention_hours, max_total_mb) = read_thresholds(state).await;
    let mut files = scan_dir(&state.cfg.images_dir).await;
    let now = chrono::Utc::now().timestamp();
    let mut deleted_count = 0u64;
    let mut freed_bytes = 0u64;

    // 1) Retention: delete files older than retention_hours.
    if retention_hours > 0 {
        let cutoff = now - (retention_hours as i64) * 3600;
        for (path, size, mtime) in &files {
            if *mtime < cutoff {
                if tokio::fs::remove_file(path).await.is_ok() {
                    deleted_count += 1;
                    freed_bytes += size;
                    tracing::info!(
                        path = %path,
                        age_s = now - mtime,
                        "cleanup: removed expired image",
                    );
                }
            }
        }
        files.retain(|(_, _, mtime)| *mtime >= cutoff);
    }

    // 2) Total size: if still over budget, delete oldest first.
    if max_total_mb > 0 {
        let limit_bytes = (max_total_mb as u64) * 1024 * 1024;
        let mut total: u64 = files.iter().map(|(_, s, _)| *s).sum();
        if total > limit_bytes {
            files.sort_by_key(|(_, _, mtime)| *mtime); // oldest first
            for (path, size, _) in files.iter() {
                if total <= limit_bytes {
                    break;
                }
                if tokio::fs::remove_file(path).await.is_ok() {
                    deleted_count += 1;
                    freed_bytes += size;
                    total -= size;
                    tracing::info!(
                        path = %path,
                        size,
                        running_total = total,
                        "cleanup: removed image to enforce total size",
                    );
                }
            }
        }
    }

    CleanupResult {
        deleted_count,
        freed_bytes,
    }
}

/// Background loop: every 10 minutes, run cleanup.
pub async fn run_cleanup_loop(state: AppState) {
    loop {
        sleep(Duration::from_secs(SCAN_INTERVAL_S)).await;
        let r = run_cleanup(&state).await;
        if r.deleted_count > 0 {
            tracing::info!(
                deleted = r.deleted_count,
                freed_bytes = r.freed_bytes,
                "periodic image cleanup finished",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scan_nonexistent_dir_returns_empty() {
        let v = scan_dir("Z:/definitely/not/a/real/path/abcxyz123").await;
        assert!(v.is_empty());
    }
}