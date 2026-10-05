mod api;
mod assets;
mod config;
mod detect;
mod events;
mod security;
mod state;
mod supervisor;
mod ws;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::{middleware, Router};
use clap::Parser;

use events::EventBus;
use state::AppState;
use supervisor::Supervisor;

#[derive(Parser)]
#[command(name = "devkit", about = "Local dev cockpit: supervises your project's processes, probes their connections, and records their traffic")]
struct Cli {
    /// Project directory to run in.
    #[arg(default_value = ".")]
    dir: String,

    /// Port to bind on 127.0.0.1. Never exposed beyond loopback.
    #[arg(short, long, default_value_t = 7777)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "devkit=info".into()))
        .init();

    let cli = Cli::parse();
    let project_dir = std::fs::canonicalize(&cli.dir)?;

    let config = config::load_or_detect(&project_dir)?;
    tracing::info!(
        "loaded {} command(s) for {}",
        config.commands.len(),
        project_dir.display()
    );
    for cmd in &config.commands {
        tracing::info!("  [{}] {} -> {} (from {})", cmd.id, cmd.name, cmd.cmd, cmd.source);
    }

    let bus = Arc::new(EventBus::new());
    let supervisor = Supervisor::new(project_dir.clone(), &config, bus.clone());

    let token = security::generate_token();
    security::persist_session(&project_dir, cli.port, &token)?;

    let app_state = AppState {
        config: Arc::new(config),
        supervisor: supervisor.clone(),
        bus,
        token: Arc::new(token),
        port: cli.port,
    };

    supervisor.auto_start_all();
    tokio::spawn(supervisor.clone().run_stats_loop());

    let api_routes = Router::new()
        .route("/state", get(api::get_state))
        .route("/commands/:id/run", post(api::run_command))
        .route("/commands/:id/stop", post(api::stop_command))
        .route("/commands/:id/restart", post(api::restart_command))
        .route_layer(middleware::from_fn_with_state(app_state.clone(), api::auth_guard));

    let app = Router::new()
        .route("/", get(assets::index))
        .route("/assets/*path", get(assets::asset))
        .route("/ws", get(ws::ws_handler))
        .nest("/api", api_routes)
        .with_state(app_state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], cli.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("devkit running at http://127.0.0.1:{}", cli.port);
    axum::serve(listener, app).await?;
    Ok(())
}
