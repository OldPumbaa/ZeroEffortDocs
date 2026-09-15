use std::path::Path;

use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::AppError;
use crate::files;
use crate::model::{
    is_valid_key, now, require_name, Field, FieldInput, FieldType, FillMode, SourceInfo,
    TemplateDetail, TemplateSummary, UpsertTemplate,
};

#[derive(sqlx::FromRow)]
struct TemplateRow {
    id: String,
    name: String,
    description: String,
    body: String,
    source_name: Option<String>,
    source_mime: Option<String>,
    created_at: String,
    updated_at: String,
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<TemplateSummary>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.name, t.description, t.created_at, t.updated_at,
               (SELECT COUNT(*) FROM template_fields f WHERE f.template_id = t.id) AS field_count,
               (SELECT COUNT(*) FROM documents d WHERE d.template_id = t.id) AS document_count
        FROM templates t
        ORDER BY t.updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(TemplateSummary {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            field_count: row.try_get("field_count")?,
            document_count: row.try_get("document_count")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(out)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<TemplateDetail, AppError> {
    let row = sqlx::query_as::<_, TemplateRow>(
        "SELECT id, name, description, body, source_name, source_mime, created_at, updated_at FROM templates WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let fields = fields_of(pool, id).await?;
    let document_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE template_id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?;
    let source = match (row.source_name, row.source_mime) {
        (Some(name), mime) if !name.is_empty() => Some(SourceInfo {
            name,
            mime: mime.unwrap_or_else(|| "application/octet-stream".into()),
        }),
        _ => None,
    };
    Ok(TemplateDetail {
        id: row.id,
        name: row.name,
        description: row.description,
        body: row.body,
        source,
        fields,
        document_count,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub async fn fields_of(pool: &SqlitePool, template_id: &str) -> Result<Vec<Field>, AppError> {
    let rows = sqlx::query(
        "SELECT id, key, label, field_type, required, fill_mode, options_json, config_json, sort_order FROM template_fields WHERE template_id = ? ORDER BY sort_order, key",
    )
    .bind(template_id)
    .fetch_all(pool)
    .await?;
    let mut fields = Vec::with_capacity(rows.len());
    for row in rows {
        let options_json: Option<String> = row.try_get("options_json")?;
        let options = match options_json {
            Some(raw) if !raw.is_empty() => serde_json::from_str(&raw)?,
            _ => Vec::new(),
        };
        let required: i64 = row.try_get("required")?;
        let type_raw: String = row.try_get("field_type")?;
        let key: String = row.try_get("key")?;
        let fill_mode = FillMode::parse(&row.try_get::<String, _>("fill_mode")?)?;
        let config_raw: String = row.try_get("config_json").unwrap_or_default();
        let (date_format, auto, seq_start) = parse_config(Some(&config_raw), fill_mode);
        let next = if fill_mode == FillMode::Sequence {
            Some(peek_seq(pool, template_id, &key, seq_start).await?)
        } else {
            None
        };
        fields.push(Field {
            id: row.try_get("id")?,
            key,
            label: row.try_get("label")?,
            field_type: FieldType::parse(&type_raw)?,
            required: required != 0,
            fill_mode,
            options,
            auto,
            date_format,
            seq_start,
            next,
        });
    }
    Ok(fields)
}

pub async fn create(pool: &SqlitePool, input: UpsertTemplate) -> Result<TemplateDetail, AppError> {
    let name = require_name(&input.name, "название шаблона")?;
    let description = input.description.trim().to_string();
    if description.len() > 2000 {
        return Err(AppError::bad("описание слишком длинное"));
    }
    let body = clamp_body(&input.body)?;
    let prepared = prepare_fields(&input.fields, None)?;
    let id = Uuid::new_v4().to_string();
    let ts = now();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO templates (id, name, description, body, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&description)
    .bind(&body)
    .bind(&ts)
    .bind(&ts)
    .execute(&mut *tx)
    .await?;
    insert_fields(&mut tx, &id, &prepared).await?;
    tx.commit().await?;
    get(pool, &id).await
}

pub async fn update(
    pool: &SqlitePool,
    id: &str,
    input: UpsertTemplate,
) -> Result<TemplateDetail, AppError> {
    let existing = get(pool, id).await?;
    let name = require_name(&input.name, "название шаблона")?;
    let description = input.description.trim().to_string();
    if description.len() > 2000 {
        return Err(AppError::bad("описание слишком длинное"));
    }
    let body = clamp_body(&input.body)?;
    let keep_ids: Vec<String> = existing.fields.iter().map(|f| f.id.clone()).collect();
    let prepared = prepare_fields(&input.fields, Some(&keep_ids))?;
    let ts = now();
    let mut tx = pool.begin().await?;
    let res = sqlx::query(
        "UPDATE templates SET name = ?, description = ?, body = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&name)
    .bind(&description)
    .bind(&body)
    .bind(&ts)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM template_fields WHERE template_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    insert_fields(&mut tx, id, &prepared).await?;
    tx.commit().await?;
    get(pool, id).await
}

pub async fn delete(pool: &SqlitePool, data_dir: &Path, id: &str) -> Result<(), AppError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE template_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Err(AppError::conflict(format!(
            "нельзя удалить шаблон: по нему уже есть документы ({count})"
        )));
    }
    let source_path: Option<String> =
        sqlx::query_scalar("SELECT source_path FROM templates WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .flatten();
    let res = sqlx::query("DELETE FROM templates WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if let Some(rel) = source_path {
        files::remove(data_dir, &rel);
    }
    Ok(())
}

pub async fn attach_source(
    pool: &SqlitePool,
    data_dir: &Path,
    id: &str,
    filename: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<TemplateDetail, AppError> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM templates WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(AppError::NotFound);
    }
    let old: Option<String> = sqlx::query_scalar("SELECT source_path FROM templates WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if let Some(rel) = old.as_deref().filter(|p| !p.is_empty()) {
        files::remove(data_dir, rel);
    }
    let ext = files::check(filename, bytes)?;
    let stored: Vec<u8> = if ext == "docx" {
        crate::extract::normalize_docx(bytes).unwrap_or_else(|err| {
            tracing::warn!(%err, "не удалось нормализовать docx, кладём как есть");
            bytes.to_vec()
        })
    } else {
        bytes.to_vec()
    };
    let (display, mime, rel) =
        files::save(data_dir, "templates", id, filename, mime, &stored).await?;
    sqlx::query(
        "UPDATE templates SET source_name = ?, source_mime = ?, source_path = ?, updated_at = ? WHERE id = ?",
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

pub async fn source_rel(pool: &SqlitePool, id: &str) -> Result<Option<String>, AppError> {
    let rel: Option<String> = sqlx::query_scalar("SELECT source_path FROM templates WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .flatten();
    Ok(rel.filter(|s| !s.is_empty()))
}

struct PreparedField {
    id: String,
    key: String,
    label: String,
    field_type: FieldType,
    required: bool,
    fill_mode: FillMode,
    options_json: Option<String>,
    config_json: String,
}

fn prepare_fields(
    inputs: &[FieldInput],
    existing_ids: Option<&[String]>,
) -> Result<Vec<PreparedField>, AppError> {
    if inputs.len() > 80 {
        return Err(AppError::bad("слишком много полей"));
    }
    let mut keys = std::collections::HashSet::new();
    let mut prepared = Vec::with_capacity(inputs.len());
    for input in inputs {
        let key = input.key.trim().to_string();
        if !is_valid_key(&key) {
            return Err(AppError::bad(format!(
                "ключ «{key}» недопустим: латиница, цифры и _, начинается с буквы"
            )));
        }
        if !keys.insert(key.clone()) {
            return Err(AppError::bad(format!("ключ «{key}» повторяется")));
        }
        let label = require_name(&input.label, "подпись поля")?;
        let auto = input.auto.unwrap_or(matches!(
            input.fill_mode,
            FillMode::CreatedAt | FillMode::Sequence
        ));
        let mut field_type = input.field_type;
        let fill_mode = if field_type == FieldType::Date && auto {
            FillMode::CreatedAt
        } else if field_type == FieldType::Number && auto {
            FillMode::Sequence
        } else if input.fill_mode == FillMode::CreatedAt {
            field_type = FieldType::Date;
            FillMode::CreatedAt
        } else if input.fill_mode == FillMode::Sequence {
            field_type = FieldType::Number;
            FillMode::Sequence
        } else {
            FillMode::Manual
        };
        let required =
            fill_mode == FillMode::Manual && input.required && field_type != FieldType::Checkbox;
        let date_format = input
            .date_format
            .as_deref()
            .unwrap_or("d.m.Y")
            .trim()
            .to_string();
        let seq_start = input.seq_start.unwrap_or(1).max(1);
        let config_json = serde_json::json!({
            "date_format": date_format,
            "auto": auto,
            "seq_start": seq_start,
        })
        .to_string();
        if field_type == FieldType::Select {
            let options: Vec<String> = input
                .options
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if options.len() < 2 {
                return Err(AppError::bad(format!(
                    "у списка «{label}» должно быть хотя бы два варианта"
                )));
            }
            let options_json = serde_json::to_string(&options)?;
            prepared.push(PreparedField {
                id: field_id(input, existing_ids),
                key,
                label,
                field_type,
                required,
                fill_mode,
                options_json: Some(options_json),
                config_json,
            });
        } else {
            prepared.push(PreparedField {
                id: field_id(input, existing_ids),
                key,
                label,
                field_type,
                required,
                fill_mode,
                options_json: None,
                config_json,
            });
        }
    }
    Ok(prepared)
}

fn field_id(input: &FieldInput, existing_ids: Option<&[String]>) -> String {
    if let (Some(id), Some(existing)) = (input.id.as_deref(), existing_ids) {
        if !id.is_empty() && existing.iter().any(|e| e == id) {
            return id.to_string();
        }
    }
    Uuid::new_v4().to_string()
}

async fn insert_fields(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    template_id: &str,
    fields: &[PreparedField],
) -> Result<(), AppError> {
    for (i, field) in fields.iter().enumerate() {
        sqlx::query(
            "INSERT INTO template_fields (id, template_id, key, label, field_type, required, fill_mode, options_json, config_json, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&field.id)
        .bind(template_id)
        .bind(&field.key)
        .bind(&field.label)
        .bind(field.field_type.as_str())
        .bind(field.required as i64)
        .bind(field.fill_mode.as_str())
        .bind(&field.options_json)
        .bind(&field.config_json)
        .bind(i as i64)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn parse_config(raw: Option<&str>, fill_mode: FillMode) -> (String, bool, i64) {
    let mut date_format = "d.m.Y".to_string();
    let mut auto = matches!(fill_mode, FillMode::CreatedAt | FillMode::Sequence);
    let mut seq_start = 1i64;
    if let Some(raw) = raw {
        if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(raw) {
            if let Some(s) = obj.get("date_format").and_then(|x| x.as_str()) {
                if !s.is_empty() {
                    date_format = s.to_string();
                }
            }
            if let Some(b) = obj.get("auto").and_then(|x| x.as_bool()) {
                auto = b;
            }
            if let Some(n) = obj.get("seq_start").and_then(|x| x.as_i64()) {
                seq_start = n.max(1);
            }
        }
    }
    (date_format, auto, seq_start)
}

async fn peek_seq(
    pool: &sqlx::SqlitePool,
    template_id: &str,
    key: &str,
    start: i64,
) -> Result<i64, AppError> {
    let v: Option<i64> = sqlx::query_scalar(
        "SELECT next_value FROM sequences WHERE template_id = ? AND field_key = ?",
    )
    .bind(template_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(v.unwrap_or(start))
}

fn clamp_body(body: &str) -> Result<String, AppError> {
    if body.len() > 200_000 {
        return Err(AppError::bad("текст шаблона слишком длинный"));
    }
    Ok(body.to_string())
}

pub fn render_body(
    layout: &str,
    fields: &[Field],
    values: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let html = crate::layout::to_html(layout);
    crate::layout::fill_placeholders(&html, fields, values)
}
