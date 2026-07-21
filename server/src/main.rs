mod api;
mod audit;
mod state;

use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

#[derive(Parser, Debug)]
#[command(name = "aipet-server", about = "AIPet intranet pet creation server")]
struct Args {
    /// Listen address
    #[arg(long, default_value = "0.0.0.0:8787")]
    bind: String,
    /// Directory for config, work dirs and artifacts
    #[arg(long, default_value = ".")]
    data_dir: PathBuf,
    /// AI config JSON path (defaults to <data_dir>/ai-config.json)
    #[arg(long)]
    ai_config: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(false)
        .init();

    let args = Args::parse();
    let data_dir = args.data_dir.canonicalize().unwrap_or(args.data_dir.clone());
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    let ai_config_path = args
        .ai_config
        .unwrap_or_else(|| data_dir.join("ai-config.json"));

    let state = Arc::new(AppState::new(data_dir.clone(), ai_config_path).expect("init state"));
    let app = api::router(state);

    let addr: SocketAddr = args.bind.parse().expect("invalid --bind address");
    tracing::info!("AIPet server listening on http://{addr}");
    tracing::info!("data dir: {}", data_dir.display());
    tracing::info!("logs: {}", data_dir.join("logs").join("server.log").display());
    tracing::info!("runs: {}", data_dir.join("runs").display());
    tracing::info!("history: {}", data_dir.join("history.jsonl").display());
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind failed");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}
