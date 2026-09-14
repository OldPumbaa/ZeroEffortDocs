use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::model::now;

pub async fn init(path: &Path) -> Result<SqlitePool, AppError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<(), AppError> {
    let stmts = [
        r#"
        CREATE TABLE IF NOT EXISTS instance (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS modules (
            id TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 0,
            enabled_at TEXT
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS template_fields (
            id TEXT PRIMARY KEY,
            template_id TEXT NOT NULL,
            key TEXT NOT NULL,
            label TEXT NOT NULL,
            field_type TEXT NOT NULL,
            required INTEGER NOT NULL DEFAULT 0,
            options_json TEXT,
            sort_order INTEGER NOT NULL,
            FOREIGN KEY (template_id) REFERENCES templates(id) ON DELETE CASCADE,
            UNIQUE (template_id, key)
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS documents (
            id TEXT PRIMARY KEY,
            template_id TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            source_name TEXT,
            source_mime TEXT,
            source_path TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (template_id) REFERENCES templates(id)
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS document_values (
            document_id TEXT NOT NULL,
            field_id TEXT NOT NULL,
            value_json TEXT NOT NULL,
            PRIMARY KEY (document_id, field_id),
            FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
            FOREIGN KEY (field_id) REFERENCES template_fields(id) ON DELETE CASCADE
        )
        "#,
    ];
    for stmt in stmts {
        sqlx::query(stmt).execute(pool).await?;
    }

    let instance_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM instance")
        .fetch_one(pool)
        .await?;
    if instance_count == 0 {
        sqlx::query("INSERT INTO instance (id, name, created_at) VALUES (?, ?, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind("Моя компания")
            .bind(now())
            .execute(pool)
            .await?;
    }

    for id in ["employees", "archive"] {
        sqlx::query("INSERT OR IGNORE INTO modules (id, enabled) VALUES (?, 0)")
            .bind(id)
            .execute(pool)
            .await?;
    }

    ensure_document_columns(pool).await?;
    Ok(())
}

async fn ensure_document_columns(pool: &SqlitePool) -> Result<(), AppError> {
    let rows = sqlx::query("PRAGMA table_info(documents)")
        .fetch_all(pool)
        .await?;
    let mut names = std::collections::HashSet::new();
    for row in rows {
        let name: String = sqlx::Row::try_get(&row, "name")?;
        names.insert(name);
    }
    if !names.contains("body") {
        sqlx::query("ALTER TABLE documents ADD COLUMN body TEXT NOT NULL DEFAULT ''")
            .execute(pool)
            .await?;
    }
    if !names.contains("source_name") {
        sqlx::query("ALTER TABLE documents ADD COLUMN source_name TEXT")
            .execute(pool)
            .await?;
    }
    if !names.contains("source_mime") {
        sqlx::query("ALTER TABLE documents ADD COLUMN source_mime TEXT")
            .execute(pool)
            .await?;
    }
    if !names.contains("source_path") {
        sqlx::query("ALTER TABLE documents ADD COLUMN source_path TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}
