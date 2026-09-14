use serde_json::{Map, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::AppError;
use crate::model::{
    now, require_name, DocumentDetail, DocumentSummary, Field, FieldType, PatchDocument,
    UpsertDocument,
};
use crate::templates;

pub async fn list(
    pool: &SqlitePool,
    template_id: Option<&str>,
) -> Result<Vec<DocumentSummary>, AppError> {
    let rows = if let Some(tid) = template_id {
        sqlx::query(
            r#"
            SELECT d.id, d.template_id, t.name AS template_name, d.title, d.created_at, d.updated_at
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
            SELECT d.id, d.template_id, t.name AS template_name, d.title, d.created_at, d.updated_at
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
        out.push(DocumentSummary {
            id: row.try_get("id")?,
            template_id: row.try_get("template_id")?,
            template_name: row.try_get("template_name")?,
            title: row.try_get("title")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(out)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<DocumentDetail, AppError> {
    let row = sqlx::query(
        "SELECT id, template_id, title, created_at, updated_at FROM documents WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let template_id: String = row.try_get("template_id")?;
    let template = templates::get(pool, &template_id).await?;
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
        template,
        values,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn create(pool: &SqlitePool, input: UpsertDocument) -> Result<DocumentDetail, AppError> {
    let template = templates::get(pool, &input.template_id).await?;
    let title = require_name(&input.title, "название документа")?;
    let normalized = normalize_values(&template.fields, &input.values)?;
    let id = Uuid::new_v4().to_string();
    let ts = now();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO documents (id, template_id, title, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&template.id)
    .bind(&title)
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
    let normalized = normalize_values(&existing.template.fields, &input.values)?;
    let ts = now();
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE documents SET title = ?, updated_at = ? WHERE id = ?")
        .bind(&title)
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

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), AppError> {
    let res = sqlx::query("DELETE FROM documents WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
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
    use crate::model::{FieldInput, UpsertTemplate};
    use crate::AppState;

    async fn state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init(&dir.path().join("t.sqlite")).await.unwrap();
        (AppState { pool }, dir)
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
