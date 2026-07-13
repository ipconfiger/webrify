//! Server entry point: load config, connect Redis, wire routes, serve with
//! graceful shutdown + per-IP rate limiting.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::middleware::from_fn_with_state;
use turnstile_server::{
    app, config::Config, rate_limit::rate_limit_middleware, rate_limit::RateLimiter,
    state::AppState, store::ChallengeStore,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::load(None)?;
    let bind_addr: SocketAddr = config.bind_addr.parse()?;
    let store = ChallengeStore::connect(&config.redis_url).await?;
    let state = AppState::new(config, store);

    // Per-IP rate limit, applied in production only — the test-facing `app()`
    // builder stays unthrottled (its oneshot requests carry no peer addr).
    // 10 req/s/IP, 1s fixed window — generous for a real user, painful for a
    // script. TODO: make env-configurable.
    let limiter = Arc::new(RateLimiter::new(Duration::from_secs(1), 10));
    let app = app(state).layer(from_fn_with_state(limiter, rate_limit_middleware));

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(%bind_addr, "webrify turnstile server listening");

    // into_make_service_with_connect_info supplies the peer SocketAddr the
    // rate-limit middleware reads via ConnectInfo<SocketAddr>.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
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
