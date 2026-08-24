//! SQLite database: pool construction, schema bootstrap, helpers.

use crate::config::Config;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

pub async fn init_pool(cfg: &Config) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = Path::new(&cfg.db_path).parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
    }

    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", cfg.db_path))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await?;

    sqlx::query(SCHEMA).execute(&pool).await?;
    ensure_kind_column(&pool).await?;
    Ok(pool)
}

/// Older DBs may be missing the `kind` column. SQLite has no
/// `ADD COLUMN IF NOT EXISTS`, so probe via PRAGMA first.
async fn ensure_kind_column(pool: &SqlitePool) -> anyhow::Result<()> {
    #[derive(sqlx::FromRow)]
    struct ColInfo {
        name: String,
    }
    let rows: Vec<ColInfo> = sqlx::query_as::<_, ColInfo>(
        "SELECT name FROM pragma_table_info('models')",
    )
    .fetch_all(pool)
    .await?;
    let has_kind = rows.iter().any(|r| r.name == "kind");
    if !has_kind {
        sqlx::query("ALTER TABLE models ADD COLUMN kind TEXT NOT NULL DEFAULT 't2i'")
            .execute(pool)
            .await?;
    }
    Ok(())
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS models (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 1,
    kind TEXT NOT NULL DEFAULT 't2i',
    workflow TEXT NOT NULL,
    mapping TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    key TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS logs (
    id TEXT PRIMARY KEY,
    ts INTEGER NOT NULL,
    provider TEXT NOT NULL,
    key_name TEXT,
    model TEXT,
    prompt TEXT,
    size TEXT,
    seed INTEGER,
    status TEXT NOT NULL,
    error TEXT,
    latency_ms INTEGER NOT NULL,
    image_urls TEXT,
    request_body TEXT,
    response_status INTEGER
);

CREATE INDEX IF NOT EXISTS idx_logs_ts ON logs(ts DESC);
CREATE INDEX IF NOT EXISTS idx_logs_status ON logs(status);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;