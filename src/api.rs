use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::model::{PatchDocument, PatchInstance, Stats, UpsertDocument, UpsertTemplate};
use crate::{documents, modules, templates, AppError, AppState};

pub fn router() -> Router<AppState> {
    Router::new().nest(
        "/api",
        Router::new()
            .route("/health", get(health))
            .route("/stats", get(stats))
            .route("/instance", get(get_instance).patch(patch_instance))
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
            .route(
                "/documents/{id}",
                get(get_document)
                    .put(update_document)
                    .delete(delete_document),
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

async fn get_instance(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let row: (String, String, String) =
        sqlx::query_as("SELECT id, name, created_at FROM instance LIMIT 1")
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(json!({
        "id": row.0,
        "name": row.1,
        "created_at": row.2,
    })))
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
    get_instance(State(state)).await
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
    documents::delete(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
