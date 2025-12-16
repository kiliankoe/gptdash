//! HTTP API endpoints for state management.
//!
//! These endpoints are used by the host UI for exporting/importing game state.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use crate::llm::{list_local_models, LlmConfig};
use crate::state::export::GameStateExport;
use crate::state::AppState;

/// Response structure for available models
#[derive(Debug, Clone, Serialize)]
pub struct AvailableModelsResponse {
    pub openai_models: Vec<ModelInfo>,
    pub ollama_models: Vec<ModelInfo>,
    pub default_model: Option<String>,
}

/// Information about a single model
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    /// Unique identifier in format "provider:model" (e.g., "openai:gpt-5-mini")
    pub id: String,
    /// Display name for the model
    pub name: String,
    /// Whether this model supports vision/image inputs
    pub supports_vision: bool,
}

/// Static list of OpenAI models to offer
const OPENAI_MODELS: &[(&str, bool)] = &[
    ("gpt-5-mini", true),
    ("gpt-5", true),
    ("gpt-4.1", true),
    ("gpt-4o", true),
    ("gpt-4o-mini", true),
    ("gpt-3.5-turbo", false), // No vision support
];

/// List available AI models.
///
/// GET /api/models
///
/// Returns models from configured providers:
/// - OpenAI: Static list (if API key is configured)
/// - Ollama: Dynamically queried from /api/tags (if base URL is configured)
pub async fn list_available_models(
    State(state): State<Arc<AppState>>,
) -> Json<AvailableModelsResponse> {
    let config = &state.llm_config;

    // Build OpenAI models list (if configured)
    let openai_models: Vec<ModelInfo> = if config.openai_api_key.is_some() {
        OPENAI_MODELS
            .iter()
            .map(|(name, vision)| ModelInfo {
                id: format!("openai:{}", name),
                name: name.to_string(),
                supports_vision: *vision,
            })
            .collect()
    } else {
        vec![]
    };

    // Build Ollama models list (if configured)
    let ollama_models: Vec<ModelInfo> = if let Some(ref base_url) = config.ollama_base_url {
        match list_local_models(base_url).await {
            Ok(models) => models
                .into_iter()
                .map(|name| {
                    // Check if it's a vision model based on name
                    let supports_vision = name.to_lowercase().contains("llava")
                        || name.to_lowercase().contains("bakllava")
                        || name.to_lowercase().contains("moondream")
                        || name.to_lowercase().contains("minicpm-v")
                        || name.to_lowercase().contains("qwen2-vl")
                        || name.to_lowercase().contains("qwen2.5-vl");
                    ModelInfo {
                        id: format!("ollama:{}", name),
                        name: name.clone(),
                        supports_vision,
                    }
                })
                .collect(),
            Err(e) => {
                tracing::warn!("Failed to list Ollama models: {}", e);
                vec![]
            }
        }
    } else {
        vec![]
    };

    // Determine default model
    let default_model = determine_default_model(config, &openai_models, &ollama_models);

    Json(AvailableModelsResponse {
        openai_models,
        ollama_models,
        default_model,
    })
}

/// Determine the default model based on configuration and available models
fn determine_default_model(
    config: &LlmConfig,
    openai_models: &[ModelInfo],
    ollama_models: &[ModelInfo],
) -> Option<String> {
    // If OpenAI is configured, use its configured model as default
    if config.openai_api_key.is_some() {
        let model_id = format!("openai:{}", config.openai_model);
        // Check if it exists in our list
        if openai_models.iter().any(|m| m.id == model_id) {
            return Some(model_id);
        }
        // If not in static list, still return it (user may have configured a different model)
        return Some(model_id);
    }

    // If Ollama is configured, use its configured model as default
    if config.ollama_base_url.is_some() {
        let model_id = format!("ollama:{}", config.ollama_model);
        if ollama_models.iter().any(|m| m.id == model_id) {
            return Some(model_id);
        }
        // Even if not in list (maybe not pulled yet), return configured model
        return Some(model_id);
    }

    None
}

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
