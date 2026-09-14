use axum::http::{header, StatusCode, Uri};
use axum::response::IntoResponse;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "web/"]
struct WebAssets;

pub async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let file = if path.is_empty() { "index.html" } else { path };
    serve(file)
}

fn serve(path: &str) -> axum::response::Response {
    match WebAssets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], file.data).into_response()
        }
        None => {
            if path.contains('.') {
                StatusCode::NOT_FOUND.into_response()
            } else if let Some(index) = WebAssets::get("index.html") {
                (
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    index.data,
                )
                    .into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }
}
