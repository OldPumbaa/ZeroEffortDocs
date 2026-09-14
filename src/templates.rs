use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::AppError;
use crate::model::{
    is_valid_key, now, require_name, Field, FieldInput, FieldType, TemplateDetail, TemplateSummary,
    UpsertTemplate,
};

#[derive(sqlx::FromRow)]
struct TemplateRow {
    id: String,
    name: String,
    description: String,
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
        "SELECT id, name, description, created_at, updated_at FROM templates WHERE id = ?",
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
    Ok(TemplateDetail {
        id: row.id,
        name: row.name,
        description: row.description,
        fields,
        document_count,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub async fn fields_of(pool: &SqlitePool, template_id: &str) -> Result<Vec<Field>, AppError> {
    let rows = sqlx::query(
        "SELECT id, key, label, field_type, required, options_json, sort_order FROM template_fields WHERE template_id = ? ORDER BY sort_order, key",
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
        fields.push(Field {
            id: row.try_get("id")?,
            key: row.try_get("key")?,
            label: row.try_get("label")?,
            field_type: FieldType::parse(&type_raw)?,
            required: required != 0,
            options,
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
    let prepared = prepare_fields(&input.fields, None)?;
    let id = Uuid::new_v4().to_string();
    let ts = now();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO templates (id, name, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&description)
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
    let keep_ids: Vec<String> = existing.fields.iter().map(|f| f.id.clone()).collect();
    let prepared = prepare_fields(&input.fields, Some(&keep_ids))?;
    let ts = now();
    let mut tx = pool.begin().await?;
    let res =
        sqlx::query("UPDATE templates SET name = ?, description = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(&description)
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

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE template_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Err(AppError::conflict(format!(
            "нельзя удалить шаблон: по нему уже есть документы ({count})"
        )));
    }
    let res = sqlx::query("DELETE FROM templates WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

struct PreparedField {
    id: String,
    key: String,
    label: String,
    field_type: FieldType,
    required: bool,
    options_json: Option<String>,
}

fn prepare_fields(
    inputs: &[FieldInput],
    existing_ids: Option<&[String]>,
) -> Result<Vec<PreparedField>, AppError> {
    if inputs.is_empty() {
        return Err(AppError::bad("добавьте хотя бы одно поле"));
    }
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
        if input.field_type == FieldType::Select {
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
                field_type: input.field_type,
                required: input.required,
                options_json: Some(options_json),
            });
        } else {
            prepared.push(PreparedField {
                id: field_id(input, existing_ids),
                key,
                label,
                field_type: input.field_type,
                required: input.required && input.field_type != FieldType::Checkbox,
                options_json: None,
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
            "INSERT INTO template_fields (id, template_id, key, label, field_type, required, options_json, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&field.id)
        .bind(template_id)
        .bind(&field.key)
        .bind(&field.label)
        .bind(field.field_type.as_str())
        .bind(field.required as i64)
        .bind(&field.options_json)
        .bind(i as i64)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
