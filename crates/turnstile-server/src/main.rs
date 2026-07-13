//! Server entry point: load config, connect Redis, wire routes, serve with
//! graceful shutdown.

use std::net::SocketAddr;

use turnstile_server::{app, config::Config, state::AppState, store::ChallengeStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::load(None)?;
    let bind_addr: SocketAddr = config.bind_addr.parse()?;
    let store = ChallengeStore::connect(&config.redis_url).await?;
    let state = AppState::new(config, store);

    let app = app(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(%bind_addr, "webrify turnstile server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("shutdown requested (Ctrl-C)"),
        _ = terminate => tracing::info!("shutdown requested (SIGTERM)"),
    }
}
