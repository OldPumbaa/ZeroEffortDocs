pub mod api;
pub mod db;
pub mod documents;
pub mod error;
pub mod model;
pub mod modules;
pub mod templates;
pub mod web;

pub use error::AppError;

use axum::Router;
use sqlx::SqlitePool;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .merge(api::router())
        .fallback(web::static_handler)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_and_home() {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::init(&dir.path().join("t.sqlite")).await.unwrap();
        let app = app(AppState { pool });

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ctype = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ctype.contains("text/html"));
    }
}
