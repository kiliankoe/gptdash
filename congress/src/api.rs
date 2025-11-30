//! HTTP API endpoints for state management.
//!
//! These endpoints are used by the host UI for exporting/importing game state.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

use crate::state::export::GameStateExport;
use crate::state::AppState;

/// Export the entire game state as JSON.
///
/// GET /api/state/export
pub async fn export_state(State(state): State<Arc<AppState>>) -> Json<GameStateExport> {
    let export = state.export_state().await;
    Json(export)
}

/// Import a game state snapshot.
///
/// POST /api/state/import
///
/// Replaces all current state with the imported data.
/// Broadcasts state refresh to all connected clients.
pub async fn import_state(
    State(state): State<Arc<AppState>>,
    Json(export): Json<GameStateExport>,
) -> Response {
    match state.import_state(export).await {
        Ok(()) => (StatusCode::OK, "State imported successfully").into_response(),
        Err(e) => {
            tracing::error!("State import failed: {}", e);
            (StatusCode::BAD_REQUEST, format!("Import failed: {}", e)).into_response()
        }
    }
}
