use axum::{
    http::StatusCode,
    middleware,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use gptdash::{
    abuse, api, auth, broadcast, llm, state::export::GameStateExport, state::AppState, ws,
};

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

    // Initialize app state
    let state = Arc::new(AppState::new_with_llm(llm_manager, llm_config));

    // Auto-load state from backup if it exists (unless disabled for tests)
    let disable_auto_save = std::env::var("DISABLE_AUTO_SAVE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    let mut loaded_from_backup = false;
    if !disable_auto_save {
        let save_path =
            std::env::var("AUTO_SAVE_PATH").unwrap_or_else(|_| "./state_backup.json".to_string());

        if let Ok(json) = tokio::fs::read_to_string(&save_path).await {
            match serde_json::from_str::<GameStateExport>(&json) {
                Ok(export) => match state.import_state(export).await {
                    Ok(()) => {
                        tracing::info!("Restored state from {}", save_path);
                        loaded_from_backup = true;
                    }
                    Err(e) => tracing::warn!("Failed to import backup: {}", e),
                },
                Err(e) => tracing::warn!("Failed to parse backup file: {}", e),
            }
        }
    }

    // Create a fresh game if we didn't load from backup
    if !loaded_from_backup {
        state.create_game().await;
    }

    // Spawn background task for auto-saving state to disk
    broadcast::spawn_auto_save_task(state.clone());

    // Spawn background task for broadcasting vote counts to Beamer
    broadcast::spawn_vote_broadcaster(state.clone());

    // Spawn background task for broadcasting prompt vote counts to Beamer during PROMPT_SELECTION
    broadcast::spawn_prompt_vote_broadcaster(state.clone());

    // Spawn background task for broadcasting connection stats to Host
    broadcast::spawn_connection_stats_broadcaster(state.clone());

    // Spawn background task for cleaning up stale rate limiter entries (prevents memory leaks)
    broadcast::spawn_rate_limiter_cleanup(abuse_config.clone());

    // Spawn background task for cleaning up stale audience members (10-min TTL, 0 points)
    broadcast::spawn_audience_cleanup_task(state.clone(), 10);

    // Spawn background task for debounced prompt pool broadcasts to host
    broadcast::spawn_prompt_broadcast_task(state.clone());

    // Spawn background task for cleaning up stale WebSocket rate limiters
    broadcast::spawn_ws_rate_limiter_cleanup_task(state.clone());

    // Note: Voting deadline is a soft/visual timer; the host advances phases manually.

    // Protected host/beamer routes (with HTTP Basic Auth)
    let host_routes = Router::new()
        .route("/host", get(auth::serve_host))
        .route("/beamer", get(auth::serve_beamer))
        .route("/api/state/export", get(api::export_state))
        .route("/api/state/import", post(api::import_state))
        .route("/api/models", get(api::list_available_models))
        .layer(middleware::from_fn_with_state(
            auth_config.clone(),
            auth::host_auth_middleware,
        ));

    // Public page routes (player only)
    // Block direct .html access - only clean routes should work
    let page_routes = Router::new()
        .route("/player", get(auth::serve_player))
        .route("/host.html", get(|| async { StatusCode::NOT_FOUND }))
        .route("/player.html", get(|| async { StatusCode::NOT_FOUND }))
        .route("/beamer.html", get(|| async { StatusCode::NOT_FOUND }));

    // WebSocket route with anti-abuse protection
    let ws_routes = Router::new()
        .route("/ws", get(ws::ws_handler))
        .layer(middleware::from_fn_with_state(
            auth_config.clone(),
            auth::host_ws_auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            abuse_config.clone(),
            abuse::ws_abuse_middleware,
        ));

    // Audience routes with panic mode blocking
    // Only `/` serves the audience page; `/index.html` always returns 404
    let audience_routes = Router::new()
        .route("/", get(auth::serve_audience))
        .route("/index.html", get(|| async { StatusCode::NOT_FOUND }))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::panic_mode_middleware,
        ));

    let app = Router::new()
        .merge(ws_routes)
        .merge(host_routes)
        .merge(page_routes)
        .merge(audience_routes)
        .fallback_service(ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // 6573 is ASCII for "AI"
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6573);
    let addr: SocketAddr = format!("{bind_addr}:{port}")
        .parse()
        .expect("Invalid BIND_ADDR/PORT");
    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
