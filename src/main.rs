use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tracing_subscriber::EnvFilter;
use zed::{app, db, AppState};

#[derive(Parser, Debug)]
#[command(
    name = "zed",
    version,
    about = "ZeroEffortDocs — документооборот без боли"
)]
struct Cli {
    /// Адрес прослушивания. 0.0.0.0:4748 — если поднимаете на сервере.
    #[arg(long, env = "ZED_BIND", default_value = "127.0.0.1:4748")]
    bind: String,

    /// Каталог данных (SQLite). По умолчанию ./data
    #[arg(long, env = "ZED_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Не открывать браузер после старта.
    #[arg(long, env = "ZED_NO_OPEN")]
    no_open: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("zed=info")),
        )
        .init();

    let cli = Cli::parse();
    let data_dir = cli.data_dir.unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("data")
    });
    let db_path = data_dir.join("zed.sqlite");

    tracing::info!(path = %db_path.display(), "opening database");
    let pool = db::init(&db_path)
        .await
        .with_context(|| format!("не удалось открыть базу {}", db_path.display()))?;

    let addr: SocketAddr = cli.bind.parse().context("некорректный --bind")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("не удалось занять {addr}"))?;
    let bound = listener.local_addr()?;

    let open_url = if bound.ip().is_unspecified() {
        format!("http://127.0.0.1:{}", bound.port())
    } else {
        format!("http://{bound}")
    };

    tracing::info!("ZED слушает {open_url}");
    if !cli.no_open {
        if let Err(err) = open::that(&open_url) {
            tracing::warn!(%err, "не получилось открыть браузер, зайдите сами: {open_url}");
        }
    }

    axum::serve(listener, app(AppState { pool }))
        .await
        .context("сервер остановился")?;
    Ok(())
}
