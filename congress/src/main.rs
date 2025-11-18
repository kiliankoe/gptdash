mod broadcast;
mod llm;
mod protocol;
mod state;
mod types;
mod ws;

use axum::{routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::state::AppState;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gptdash=debug,tower_http=debug,axum=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting GPTDash...");

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

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
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
