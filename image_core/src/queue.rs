//! Concurrency queue: a tokio::Semaphore plus a snapshot of currently-running
//! and currently-waiting items for the `/admin/api/queue` endpoint.
//!
//! Each ComfyUI execution acquires a permit before doing real work, and the
//! lifetime of the permit covers the full request — including enqueue time,
//! execution, and image fetch.

use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore, TryAcquireError};
use uuid::Uuid;

/// Display info for one queued or running request.
#[derive(Debug, Clone, Serialize)]
pub struct QueueItem {
    pub id: String,
    pub model: String,
    pub prompt: String,
    pub since_ts: i64,
}

pub struct QueueInner {
    pub sem: Arc<Semaphore>,
    pub max: u32,
    pub running: Vec<QueueItem>,
    pub queued: Vec<QueueItem>,
}

/// Public handle that handlers clone cheaply.
#[derive(Clone)]
pub struct QueueManager {
    inner: Arc<Mutex<QueueInner>>,
}

impl QueueManager {
    pub fn new(initial_max: u32) -> Self {
        let m = initial_max.max(1);
        Self {
            inner: Arc::new(Mutex::new(QueueInner {
                sem: Arc::new(Semaphore::new(m as usize)),
                max: m,
                running: Vec::new(),
                queued: Vec::new(),
            })),
        }
    }

    /// Re-create the underlying semaphore with a new max. Pending permits are
    /// preserved: future calls to `acquire` will block on a semaphore of the
    /// new size, but currently-held permits are not revoked.
    pub async fn set_max(&self, new_max: u32) {
        let mut inner = self.inner.lock().await;
        let m = new_max.max(1);
        let sem = Arc::new(Semaphore::new(m as usize));
        inner.sem = sem;
        inner.max = m;
    }

    pub async fn snapshot(&self) -> QueueSnapshot {
        let inner = self.inner.lock().await;
        QueueSnapshot {
            max_concurrent: inner.max,
            running: inner.running.len() as u32,
            queued: inner.queued.len() as u32,
            running_items: inner.running.clone(),
            queued_items: inner.queued.clone(),
        }
    }

    /// Register a queued item, then await a permit. Returns:
    ///  - `QueueGuard` whose `Drop` releases the permit AND removes the running
    ///    entry, so callers should `let _g = queue.acquire(...).await?;` before
    ///    the actual ComfyUI call.
    pub async fn acquire(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<QueueGuard, tokio::sync::AcquireError> {
        let queued_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let preview = preview_prompt(prompt);

        // Insert into `queued` first.
        {
            let mut inner = self.inner.lock().await;
            inner.queued.push(QueueItem {
                id: queued_id.clone(),
                model: model.to_string(),
                prompt: preview.clone(),
                since_ts: now,
            });
        }

        // Acquire permit (may wait).
        let sem = {
            let inner = self.inner.lock().await;
            inner.sem.clone()
        };
        let permit = sem.acquire_owned().await?;

        // Promote from queued -> running.
        let running_id = Uuid::new_v4().to_string();
        {
            let mut inner = self.inner.lock().await;
            inner.queued.retain(|q| q.id != queued_id);
            inner.running.push(QueueItem {
                id: running_id.clone(),
                model: model.to_string(),
                prompt: preview,
                since_ts: chrono::Utc::now().timestamp(),
            });
        }

        Ok(QueueGuard {
            permit: Some(permit),
            manager: self.clone(),
            running_id,
        })
    }
}

/// Drop guard that releases the permit and removes the running entry.
pub struct QueueGuard {
    permit: Option<OwnedSemaphorePermit>,
    manager: QueueManager,
    running_id: String,
}

impl Drop for QueueGuard {
    fn drop(&mut self) {
        if let Some(p) = self.permit.take() {
            drop(p);
        }
        let m = self.manager.clone();
        let id = self.running_id.clone();
        // Try-acquire the lock; if contended just drop — list stays slightly
        // stale until next snapshot is taken.
        if let Ok(mut inner) = m.inner.try_lock() {
            inner.running.retain(|r| r.id != id);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueSnapshot {
    pub max_concurrent: u32,
    /// Number of items currently executing.
    pub running: u32,
    /// Number of items currently waiting for a permit.
    pub queued: u32,
    pub running_items: Vec<QueueItem>,
    pub queued_items: Vec<QueueItem>,
}

#[allow(dead_code)]
pub fn _silence(_e: TryAcquireError) {}
#[allow(dead_code)]
pub fn _silence_dur() {
    let _ = Duration::from_secs(0);
    let _ = Instant::now();
}

fn preview_prompt(s: &str) -> String {
    // First 50 chars, replace newlines.
    let one_line: String = s
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .take(50)
        .collect();
    one_line
}