use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header::{self, HeaderValue};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::model::{
    ImportDocument, PatchDocument, PatchInstance, SetupInput, Stats, UpsertDocument, UpsertTemplate,
};
use crate::{documents, extract, modules, templates, AppError, AppState};

pub fn router() -> Router<AppState> {
    Router::new().nest(
        "/api",
        Router::new()
            .route("/health", get(health))
            .route("/stats", get(stats))
            .route("/instance", get(get_instance).patch(patch_instance))
            .route("/setup", post(setup_instance))
            .route("/extract", post(extract_file))
            .route("/modules", get(list_modules))
            .route("/modules/{id}/enable", post(enable_module))
            .route("/modules/{id}/disable", post(disable_module))
            .route("/templates", get(list_templates).post(create_template))
            .route(
                "/templates/{id}",
                get(get_template)
                    .put(update_template)
                    .delete(delete_template),
            )
            .route("/documents", get(list_documents).post(create_document))
            .route("/import", post(import_document))
            .route(
                "/documents/{id}",
                get(get_document)
                    .put(update_document)
                    .delete(delete_document),
            )
            .route(
                "/documents/{id}/source",
                get(get_source).put(put_source).delete(delete_source),
            ),
    )
}

async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "name": "zed",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn stats(State(state): State<AppState>) -> Result<Json<Stats>, AppError> {
    let templates: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM templates")
        .fetch_one(&state.pool)
        .await?;
    let documents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&state.pool)
        .await?;
    let modules_enabled: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM modules WHERE enabled = 1")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(Stats {
        templates,
        documents,
        modules_enabled,
    }))
}

async fn instance_json(pool: &sqlx::SqlitePool) -> Result<Json<Value>, AppError> {
    let row: (String, String, i64, String) =
        sqlx::query_as("SELECT id, name, setup_done, created_at FROM instance LIMIT 1")
            .fetch_one(pool)
            .await?;
    let modules = modules::list(pool).await?;
    Ok(Json(json!({
        "id": row.0,
        "name": row.1,
        "setup_done": row.2 != 0,
        "created_at": row.3,
        "modules": modules,
    })))
}

async fn get_instance(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    instance_json(&state.pool).await
}

async fn setup_instance(
    State(state): State<AppState>,
    Json(body): Json<SetupInput>,
) -> Result<Json<Value>, AppError> {
    let name = crate::model::require_name(&body.name, "название компании")?;
    modules::set_enabled_set(&state.pool, &body.enabled_modules).await?;
    sqlx::query("UPDATE instance SET name = ?, setup_done = 1")
        .bind(&name)
        .execute(&state.pool)
        .await?;
    instance_json(&state.pool).await
}

async fn patch_instance(
    State(state): State<AppState>,
    Json(body): Json<PatchInstance>,
) -> Result<Json<Value>, AppError> {
    let name = crate::model::require_name(&body.name, "название компании")?;
    sqlx::query("UPDATE instance SET name = ?")
        .bind(&name)
        .execute(&state.pool)
        .await?;
    if let Some(ids) = body.enabled_modules {
        modules::set_enabled_set(&state.pool, &ids).await?;
    }
    instance_json(&state.pool).await
}

async fn extract_file(mut multipart: Multipart) -> Result<Json<Value>, AppError> {
    let mut found = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad(e.to_string()))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let filename = field.file_name().unwrap_or("file").to_string();
        let bytes = field
            .bytes()
            .await
            .map_err(|e| AppError::bad(e.to_string()))?;
        found = Some((filename, bytes));
    }
    let Some((filename, bytes)) = found else {
        return Err(AppError::bad("приложите файл"));
    };
    let extracted = extract::from_bytes(&filename, &bytes)?;
    Ok(Json(json!(extracted)))
}

async fn list_modules(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(modules::list(&state.pool).await?)))
}

async fn enable_module(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(
        modules::set_enabled(&state.pool, &id, true).await?
    )))
}

async fn disable_module(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(
        modules::set_enabled(&state.pool, &id, false).await?
    )))
}

async fn list_templates(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(templates::list(&state.pool).await?)))
}

async fn create_template(
    State(state): State<AppState>,
    Json(body): Json<UpsertTemplate>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let created = templates::create(&state.pool, body).await?;
    Ok((StatusCode::CREATED, Json(json!(created))))
}

async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(templates::get(&state.pool, &id).await?)))
}

async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpsertTemplate>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(
        templates::update(&state.pool, &id, body).await?
    )))
}

async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    templates::delete(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct DocQuery {
    template_id: Option<String>,
}

async fn list_documents(
    State(state): State<AppState>,
    Query(q): Query<DocQuery>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(
        documents::list(&state.pool, q.template_id.as_deref()).await?
    )))
}

async fn create_document(
    State(state): State<AppState>,
    Json(body): Json<UpsertDocument>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let created = documents::create(&state.pool, body).await?;
    Ok((StatusCode::CREATED, Json(json!(created))))
}

async fn import_document(
    State(state): State<AppState>,
    Json(body): Json<ImportDocument>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let created = documents::import(&state.pool, body).await?;
    Ok((StatusCode::CREATED, Json(json!(created))))
}

async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(documents::get(&state.pool, &id).await?)))
}

async fn update_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PatchDocument>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(
        documents::update(&state.pool, &id, body).await?
    )))
}

async fn delete_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    documents::delete(&state.pool, &state.data_dir, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let (name, mime, bytes) = documents::source_bytes(&state.pool, &state.data_dir, &id).await?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&mime)
                    .unwrap_or(HeaderValue::from_static("application/octet-stream")),
            ),
            (header::CONTENT_DISPOSITION, content_disposition(&name)),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            ),
        ],
        Body::from(bytes),
    ))
}

async fn put_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<Value>, AppError> {
    let mut found = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad(e.to_string()))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let filename = field.file_name().unwrap_or("file").to_string();
        let mime = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = field
            .bytes()
            .await
            .map_err(|e| AppError::bad(e.to_string()))?;
        found = Some((filename, mime, bytes));
    }
    let Some((filename, mime, bytes)) = found else {
        return Err(AppError::bad("приложите файл"));
    };
    let doc = documents::attach_source(&state.pool, &state.data_dir, &id, &filename, &mime, &bytes)
        .await?;
    Ok(Json(json!(doc)))
}

async fn delete_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!(
        documents::clear_source(&state.pool, &state.data_dir, &id).await?
    )))
}

fn content_disposition(name: &str) -> HeaderValue {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    HeaderValue::from_str(&format!(
        "attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}
