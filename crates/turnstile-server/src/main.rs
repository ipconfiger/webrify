//! Entry point. Dispatches to either the CLI (`webrify sitekey ...`) or the
//! async verification server.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::middleware::from_fn_with_state;
use turnstile_server::{
    app, config::Config, rate_limit::rate_limit_middleware, rate_limit::RateLimiter,
    state::AppState, store::ChallengeStore,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    // Subcommand: `webrify sitekey <add|remove|list> [origin] [--config <path>]`
    if args.get(1).map(String::as_str) == Some("sitekey") {
        return turnstile_server::cli::sitekey(&args[2..]);
    }
    run_server()
}

#[tokio::main]
async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::load(None)?;
    let bind_addr: SocketAddr = config.bind_addr.parse()?;
    let store = match &config.cluster_urls {
        Some(urls) => ChallengeStore::connect_cluster(urls).await?,
        None => ChallengeStore::connect_single(&config.redis_url).await?,
    };
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
