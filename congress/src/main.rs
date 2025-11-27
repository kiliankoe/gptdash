use axum::{middleware, routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use gptdash::{abuse, auth, broadcast, llm, state::AppState, ws};

#[tokio::main]
async fn main() {
    // Load .env file if present (before any env var reads)
    if let Err(e) = dotenvy::dotenv() {
        // Not an error if .env doesn't exist, only log if it's a different issue
        if !matches!(e, dotenvy::Error::Io(_)) {
            eprintln!("Warning: Failed to load .env file: {}", e);
        }
    }

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gptdash=debug,tower_http=debug,axum=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting GPTDash...");

    // Initialize authentication config
    let auth_config = Arc::new(auth::AuthConfig::from_env());

    // Initialize anti-abuse config
    let abuse_config = Arc::new(abuse::AbuseConfig::from_env());

    // Initialize LLM providers
    let llm_config = llm::LlmConfig::from_env();
    let llm_manager = match llm_config.build_manager() {
        Ok(manager) => {
            tracing::info!("LLM providers initialized successfully");
            Some(manager)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to initialize LLM providers: {}. AI answers will not be available.",
                e
            );
            None
        }
    };

    // Initialize a default game
    let state = Arc::new(AppState::new_with_llm(llm_manager, llm_config));
    state.create_game().await;

    // Spawn background task for broadcasting vote counts to Beamer
    broadcast::spawn_vote_broadcaster(state.clone());

    // Spawn background task for auto-advancing from VOTING to RESULTS when deadline expires
    broadcast::spawn_voting_deadline_watcher(state.clone());

    // Protected host routes (with HTTP Basic Auth)
    let host_routes = Router::new()
        .route("/host.html", get(auth::serve_host_html))
        .layer(middleware::from_fn_with_state(
            auth_config.clone(),
            auth::host_auth_middleware,
        ));

    // WebSocket route with anti-abuse protection
    let ws_routes =
        Router::new()
            .route("/ws", get(ws::ws_handler))
            .layer(middleware::from_fn_with_state(
                abuse_config.clone(),
                abuse::ws_abuse_middleware,
            ));

    let app = Router::new()
        .merge(ws_routes)
        .merge(host_routes)
        .fallback_service(ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // 6573 is ascii for "AI"
    let addr = SocketAddr::from(([0, 0, 0, 0], 6573));
    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
