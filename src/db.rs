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
            setup_done INTEGER NOT NULL DEFAULT 0,
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
            body TEXT NOT NULL DEFAULT '',
            source_name TEXT,
            source_mime TEXT,
            source_path TEXT,
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
            fill_mode TEXT NOT NULL DEFAULT 'manual',
            config_json TEXT NOT NULL DEFAULT '{}',
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
        CREATE TABLE IF NOT EXISTS sequences (
            template_id TEXT NOT NULL,
            field_key TEXT NOT NULL,
            next_value INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (template_id, field_key)
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
        sqlx::query("INSERT INTO instance (id, name, setup_done, created_at) VALUES (?, ?, 0, ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind("")
            .bind(now())
            .execute(pool)
            .await?;
    }

    for id in ["employees", "archive", "libreoffice"] {
        sqlx::query("INSERT OR IGNORE INTO modules (id, enabled) VALUES (?, 0)")
            .bind(id)
            .execute(pool)
            .await?;
    }

    ensure_extra_columns(pool).await?;
    Ok(())
}

async fn table_columns(
    pool: &SqlitePool,
    table: &str,
) -> Result<std::collections::HashSet<String>, AppError> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    let mut names = std::collections::HashSet::new();
    for row in rows {
        let name: String = sqlx::Row::try_get(&row, "name")?;
        names.insert(name);
    }
    Ok(names)
}

async fn ensure_extra_columns(pool: &SqlitePool) -> Result<(), AppError> {
    let inst = table_columns(pool, "instance").await?;
    if !inst.contains("setup_done") {
        sqlx::query("ALTER TABLE instance ADD COLUMN setup_done INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
    }

    let tmpl = table_columns(pool, "templates").await?;
    if !tmpl.contains("body") {
        sqlx::query("ALTER TABLE templates ADD COLUMN body TEXT NOT NULL DEFAULT ''")
            .execute(pool)
            .await?;
    }
    if !tmpl.contains("source_name") {
        sqlx::query("ALTER TABLE templates ADD COLUMN source_name TEXT")
            .execute(pool)
            .await?;
    }
    if !tmpl.contains("source_mime") {
        sqlx::query("ALTER TABLE templates ADD COLUMN source_mime TEXT")
            .execute(pool)
            .await?;
    }
    if !tmpl.contains("source_path") {
        sqlx::query("ALTER TABLE templates ADD COLUMN source_path TEXT")
            .execute(pool)
            .await?;
    }

    let fields = table_columns(pool, "template_fields").await?;
    if !fields.contains("config_json") {
        sqlx::query(
            "ALTER TABLE template_fields ADD COLUMN config_json TEXT NOT NULL DEFAULT '{}'",
        )
        .execute(pool)
        .await?;
    }
    if !fields.contains("fill_mode") {
        sqlx::query(
            "ALTER TABLE template_fields ADD COLUMN fill_mode TEXT NOT NULL DEFAULT 'manual'",
        )
        .execute(pool)
        .await?;
    }

    let docs = table_columns(pool, "documents").await?;
    if !docs.contains("body") {
        sqlx::query("ALTER TABLE documents ADD COLUMN body TEXT NOT NULL DEFAULT ''")
            .execute(pool)
            .await?;
    }
    if !docs.contains("source_name") {
        sqlx::query("ALTER TABLE documents ADD COLUMN source_name TEXT")
            .execute(pool)
            .await?;
    }
    if !docs.contains("source_mime") {
        sqlx::query("ALTER TABLE documents ADD COLUMN source_mime TEXT")
            .execute(pool)
            .await?;
    }
    if !docs.contains("source_path") {
        sqlx::query("ALTER TABLE documents ADD COLUMN source_path TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}
