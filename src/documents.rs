use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::AppError;
use crate::model::{
    now, require_name, DocumentDetail, DocumentSummary, Field, FieldType, ImportDocument,
    PatchDocument, SourceInfo, UpsertDocument, UpsertTemplate,
};
use crate::templates;

pub const MAX_SOURCE_BYTES: usize = 20 * 1024 * 1024;
const BLOCKED_EXT: &[&str] = &[
    "exe", "bat", "cmd", "com", "dll", "msi", "scr", "ps1", "vbs",
];

pub async fn list(
    pool: &SqlitePool,
    template_id: Option<&str>,
) -> Result<Vec<DocumentSummary>, AppError> {
    let rows = if let Some(tid) = template_id {
        sqlx::query(
            r#"
            SELECT d.id, d.template_id, t.name AS template_name, d.title, d.created_at, d.updated_at,
                   d.source_path
            FROM documents d
            JOIN templates t ON t.id = d.template_id
            WHERE d.template_id = ?
            ORDER BY d.updated_at DESC
            "#,
        )
        .bind(tid)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT d.id, d.template_id, t.name AS template_name, d.title, d.created_at, d.updated_at,
                   d.source_path
            FROM documents d
            JOIN templates t ON t.id = d.template_id
            ORDER BY d.updated_at DESC
            "#,
        )
        .fetch_all(pool)
        .await?
    };
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let source_path: Option<String> = row.try_get("source_path")?;
        out.push(DocumentSummary {
            id: row.try_get("id")?,
            template_id: row.try_get("template_id")?,
            template_name: row.try_get("template_name")?,
            title: row.try_get("title")?,
            has_source: source_path.as_deref().is_some_and(|p| !p.is_empty()),
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(out)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<DocumentDetail, AppError> {
    let row = sqlx::query(
        "SELECT id, template_id, title, body, source_name, source_mime, created_at, updated_at FROM documents WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let template_id: String = row.try_get("template_id")?;
    let template = templates::get(pool, &template_id).await?;
    let source_name: Option<String> = row.try_get("source_name")?;
    let source_mime: Option<String> = row.try_get("source_mime")?;
    let source = match (source_name, source_mime) {
        (Some(name), mime) if !name.is_empty() => Some(SourceInfo {
            name,
            mime: mime.unwrap_or_else(|| "application/octet-stream".into()),
        }),
        _ => None,
    };
    let value_rows = sqlx::query(
        r#"
        SELECT f.key, v.value_json
        FROM document_values v
        JOIN template_fields f ON f.id = v.field_id
        WHERE v.document_id = ?
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut values = Map::new();
    for vr in value_rows {
        let key: String = vr.try_get("key")?;
        let raw: String = vr.try_get("value_json")?;
        values.insert(key, serde_json::from_str(&raw)?);
    }
    Ok(DocumentDetail {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        body: row.try_get("body")?,
        source,
        template,
        values,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn create(pool: &SqlitePool, input: UpsertDocument) -> Result<DocumentDetail, AppError> {
    let template = templates::get(pool, &input.template_id).await?;
    let title = require_name(&input.title, "название документа")?;
    let body = clamp_body(&input.body)?;
    let normalized = normalize_values(&template.fields, &input.values)?;
    let id = Uuid::new_v4().to_string();
    let ts = now();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO documents (id, template_id, title, body, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&template.id)
    .bind(&title)
    .bind(&body)
    .bind(&ts)
    .bind(&ts)
    .execute(&mut *tx)
    .await?;
    write_values(&mut tx, &id, &template.fields, &normalized).await?;
    tx.commit().await?;
    get(pool, &id).await
}

pub async fn update(
    pool: &SqlitePool,
    id: &str,
    input: PatchDocument,
) -> Result<DocumentDetail, AppError> {
    let existing = get(pool, id).await?;
    let title = require_name(&input.title, "название документа")?;
    let body = clamp_body(&input.body)?;
    let normalized = normalize_values(&existing.template.fields, &input.values)?;
    let ts = now();
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE documents SET title = ?, body = ?, updated_at = ? WHERE id = ?")
        .bind(&title)
        .bind(&body)
        .bind(&ts)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM document_values WHERE document_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    write_values(&mut tx, id, &existing.template.fields, &normalized).await?;
    tx.commit().await?;
    get(pool, id).await
}

pub async fn delete(pool: &SqlitePool, data_dir: &Path, id: &str) -> Result<(), AppError> {
    let source_path: Option<String> =
        sqlx::query_scalar("SELECT source_path FROM documents WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .flatten();
    let res = sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if let Some(rel) = source_path {
        remove_stored(data_dir, &rel);
    }
    Ok(())
}

pub async fn import(pool: &SqlitePool, input: ImportDocument) -> Result<DocumentDetail, AppError> {
    let template = if let Some(form_id) = input.form_id.as_deref().filter(|s| !s.is_empty()) {
        templates::get(pool, form_id).await?
    } else {
        let name = require_name(input.form_name.as_deref().unwrap_or(""), "название формы")?;
        templates::create(
            pool,
            UpsertTemplate {
                name,
                description: String::new(),
                fields: input.fields,
            },
        )
        .await?
    };
    create(
        pool,
        UpsertDocument {
            template_id: template.id,
            title: input.title,
            body: input.body,
            values: input.values,
        },
    )
    .await
}

pub async fn attach_source(
    pool: &SqlitePool,
    data_dir: &Path,
    id: &str,
    filename: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<DocumentDetail, AppError> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(AppError::bad("файл больше 20 МБ"));
    }
    let ext = extension_of(filename);
    if BLOCKED_EXT.contains(&ext.as_str()) {
        return Err(AppError::bad("этот тип файла нельзя импортировать"));
    }
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(AppError::NotFound);
    }
    let old: Option<String> = sqlx::query_scalar("SELECT source_path FROM documents WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if let Some(rel) = old.as_deref().filter(|p| !p.is_empty()) {
        remove_stored(data_dir, rel);
    }

    let disk_name = format!(
        "original{}",
        if ext.is_empty() {
            String::new()
        } else {
            format!(".{ext}")
        }
    );
    let rel = format!("files/{id}/{disk_name}");
    let abs = data_dir.join(&rel);
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&abs, bytes).await?;

    let display = display_name(filename);
    let mime = if mime.trim().is_empty() {
        "application/octet-stream".to_string()
    } else {
        mime.trim().to_string()
    };
    sqlx::query(
        "UPDATE documents SET source_name = ?, source_mime = ?, source_path = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&display)
    .bind(&mime)
    .bind(&rel)
    .bind(now())
    .bind(id)
    .execute(pool)
    .await?;
    get(pool, id).await
}

pub async fn source_bytes(
    pool: &SqlitePool,
    data_dir: &Path,
    id: &str,
) -> Result<(String, String, Vec<u8>), AppError> {
    let row =
        sqlx::query("SELECT source_name, source_mime, source_path FROM documents WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or(AppError::NotFound)?;
    let name: Option<String> = row.try_get("source_name")?;
    let mime: Option<String> = row.try_get("source_mime")?;
    let rel: Option<String> = row.try_get("source_path")?;
    let (Some(name), Some(rel)) = (name, rel) else {
        return Err(AppError::NotFound);
    };
    if rel.is_empty() || Path::new(&rel).is_absolute() || rel.contains("..") {
        return Err(AppError::NotFound);
    }
    let bytes = tokio::fs::read(data_dir.join(&rel)).await?;
    Ok((
        name,
        mime.unwrap_or_else(|| "application/octet-stream".into()),
        bytes,
    ))
}

pub async fn clear_source(
    pool: &SqlitePool,
    data_dir: &Path,
    id: &str,
) -> Result<DocumentDetail, AppError> {
    let old: Option<String> = sqlx::query_scalar("SELECT source_path FROM documents WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .flatten();
    let res = sqlx::query(
        "UPDATE documents SET source_name = NULL, source_mime = NULL, source_path = NULL, updated_at = ? WHERE id = ?",
    )
    .bind(now())
    .bind(id)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if let Some(rel) = old.as_deref().filter(|p| !p.is_empty()) {
        remove_stored(data_dir, rel);
    }
    get(pool, id).await
}

fn clamp_body(body: &str) -> Result<String, AppError> {
    if body.len() > 200_000 {
        return Err(AppError::bad("текст документа слишком длинный"));
    }
    Ok(body.to_string())
}

fn extension_of(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn display_name(filename: &str) -> String {
    Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .chars()
        .take(180)
        .collect()
}

fn remove_stored(data_dir: &Path, rel: &str) {
    if rel.is_empty() || Path::new(rel).is_absolute() || rel.contains("..") {
        return;
    }
    let abs: PathBuf = data_dir.join(rel);
    let _ = std::fs::remove_file(&abs);
    if let Some(parent) = abs.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

fn normalize_values(
    fields: &[Field],
    values: &Map<String, Value>,
) -> Result<Vec<(String, Value)>, AppError> {
    let mut out = Vec::with_capacity(fields.len());
    for field in fields {
        let raw = values.get(&field.key).cloned().unwrap_or(Value::Null);
        out.push((field.id.clone(), validate_value(field, raw)?));
    }
    Ok(out)
}

fn validate_value(field: &Field, value: Value) -> Result<Value, AppError> {
    if value.is_null() {
        if field.required {
            return Err(AppError::bad(format!("поле «{}» обязательно", field.label)));
        }
        return Ok(Value::Null);
    }
    match field.field_type {
        FieldType::Text | FieldType::Textarea | FieldType::Date | FieldType::Select => {
            let Some(s) = value.as_str() else {
                return Err(AppError::bad(format!(
                    "поле «{}» должно быть текстом",
                    field.label
                )));
            };
            let s = s.trim();
            if s.is_empty() {
                if field.required {
                    return Err(AppError::bad(format!("поле «{}» обязательно", field.label)));
                }
                return Ok(Value::Null);
            }
            if field.field_type == FieldType::Text && s.len() > 500 {
                return Err(AppError::bad(format!(
                    "поле «{}» слишком длинное",
                    field.label
                )));
            }
            if field.field_type == FieldType::Textarea && s.len() > 20_000 {
                return Err(AppError::bad(format!(
                    "поле «{}» слишком длинное",
                    field.label
                )));
            }
            if field.field_type == FieldType::Date && !is_iso_date(s) {
                return Err(AppError::bad(format!(
                    "поле «{}» должно быть датой ГГГГ-ММ-ДД",
                    field.label
                )));
            }
            if field.field_type == FieldType::Select && !field.options.iter().any(|o| o == s) {
                return Err(AppError::bad(format!(
                    "поле «{}»: нет такого варианта",
                    field.label
                )));
            }
            Ok(Value::String(s.to_string()))
        }
        FieldType::Number => {
            let n = match &value {
                Value::Number(num) => num.as_f64(),
                Value::String(s) if s.trim().is_empty() => {
                    if field.required {
                        return Err(AppError::bad(format!("поле «{}» обязательно", field.label)));
                    }
                    return Ok(Value::Null);
                }
                Value::String(s) => s.trim().replace(',', ".").parse().ok(),
                _ => None,
            };
            let Some(n) = n else {
                return Err(AppError::bad(format!(
                    "поле «{}» должно быть числом",
                    field.label
                )));
            };
            Ok(serde_json::json!(n))
        }
        FieldType::Checkbox => Ok(Value::Bool(match value {
            Value::Bool(b) => b,
            Value::String(s) => s == "true" || s == "1" || s == "on",
            Value::Number(n) => n.as_i64() == Some(1),
            _ => false,
        })),
    }
}

fn is_iso_date(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && s.chars().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        })
}

async fn write_values(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    document_id: &str,
    fields: &[Field],
    values: &[(String, Value)],
) -> Result<(), AppError> {
    for (field_id, value) in values {
        if value.is_null() {
            continue;
        }
        if !fields.iter().any(|f| f.id == *field_id) {
            continue;
        }
        let raw = serde_json::to_string(value)?;
        sqlx::query(
            "INSERT INTO document_values (document_id, field_id, value_json) VALUES (?, ?, ?)",
        )
        .bind(document_id)
        .bind(field_id)
        .bind(raw)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::model::{FieldInput, FieldType, ImportDocument, UpsertTemplate};
    use crate::AppState;

    async fn state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init(&dir.path().join("t.sqlite")).await.unwrap();
        (
            AppState {
                pool,
                data_dir: dir.path().to_path_buf(),
            },
            dir,
        )
    }

    fn hire_template() -> UpsertTemplate {
        UpsertTemplate {
            name: "Приём на работу".into(),
            description: "демо".into(),
            fields: vec![
                FieldInput {
                    id: None,
                    key: "full_name".into(),
                    label: "ФИО".into(),
                    field_type: FieldType::Text,
                    required: true,
                    options: vec![],
                },
                FieldInput {
                    id: None,
                    key: "start_date".into(),
                    label: "Дата".into(),
                    field_type: FieldType::Date,
                    required: true,
                    options: vec![],
                },
                FieldInput {
                    id: None,
                    key: "kind".into(),
                    label: "Тип".into(),
                    field_type: FieldType::Select,
                    required: false,
                    options: vec!["Трудовой".into(), "ГПХ".into()],
                },
            ],
        }
    }

    #[tokio::test]
    async fn create_and_fill() {
        let (state, _dir) = state().await;
        let tmpl = templates::create(&state.pool, hire_template())
            .await
            .unwrap();
        assert_eq!(tmpl.fields.len(), 3);

        let err = create(
            &state.pool,
            UpsertDocument {
                template_id: tmpl.id.clone(),
                title: "Иванов".into(),
                body: String::new(),
                values: Map::new(),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("обязательно"));

        let mut values = Map::new();
        values.insert("full_name".into(), Value::String("Иванов Иван".into()));
        values.insert("start_date".into(), Value::String("2026-09-14".into()));
        values.insert("kind".into(), Value::String("Трудовой".into()));
        let doc = create(
            &state.pool,
            UpsertDocument {
                template_id: tmpl.id.clone(),
                title: "Иванов И.И.".into(),
                body: "черновик приказа".into(),
                values,
            },
        )
        .await
        .unwrap();
        assert_eq!(doc.values["full_name"], "Иванов Иван");

        let listed = list(&state.pool, None).await.unwrap();
        assert_eq!(listed.len(), 1);

        let del = templates::delete(&state.pool, &tmpl.id).await.unwrap_err();
        assert!(matches!(del, AppError::Conflict(_)));
        assert_eq!(doc.body, "черновик приказа");
    }

    #[tokio::test]
    async fn import_builds_form() {
        let (state, _dir) = state().await;
        let mut values = Map::new();
        values.insert("full_name".into(), Value::String("Петров".into()));
        values.insert("start_date".into(), Value::String("2026-01-10".into()));
        let doc = import(
            &state.pool,
            ImportDocument {
                title: "Петров П.П.".into(),
                body: "текст".into(),
                form_id: None,
                form_name: Some("Приём на работу".into()),
                fields: hire_template().fields,
                values,
            },
        )
        .await
        .unwrap();
        assert_eq!(doc.template.name, "Приём на работу");
        assert_eq!(doc.template.fields.len(), 3);
        assert_eq!(doc.body, "текст");

        let again = import(
            &state.pool,
            ImportDocument {
                title: "Сидоров".into(),
                body: String::new(),
                form_id: Some(doc.template.id.clone()),
                form_name: None,
                fields: vec![],
                values: {
                    let mut v = Map::new();
                    v.insert("full_name".into(), Value::String("Сидоров".into()));
                    v.insert("start_date".into(), Value::String("2026-02-01".into()));
                    v
                },
            },
        )
        .await
        .unwrap();
        assert_eq!(again.template.id, doc.template.id);
        assert_eq!(templates::list(&state.pool).await.unwrap().len(), 1);

        let attached = attach_source(
            &state.pool,
            &state.data_dir,
            &doc.id,
            "scan.pdf",
            "application/pdf",
            b"%PDF-1.4 demo",
        )
        .await
        .unwrap();
        assert_eq!(attached.source.as_ref().unwrap().name, "scan.pdf");

        let exe = attach_source(
            &state.pool,
            &state.data_dir,
            &doc.id,
            "virus.exe",
            "application/octet-stream",
            b"MZ",
        )
        .await
        .unwrap_err();
        assert!(exe.to_string().contains("нельзя"));
    }

    #[tokio::test]
    async fn reject_bad_select() {
        let (state, _dir) = state().await;
        let err = templates::create(
            &state.pool,
            UpsertTemplate {
                name: "X".into(),
                description: String::new(),
                fields: vec![FieldInput {
                    id: None,
                    key: "kind".into(),
                    label: "Тип".into(),
                    field_type: FieldType::Select,
                    required: false,
                    options: vec!["один".into()],
                }],
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("два варианта"));
    }
}
