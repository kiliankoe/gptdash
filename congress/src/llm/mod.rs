mod ollama;
mod openai;

use async_trait::async_trait;
use std::time::Duration;

pub use ollama::{list_local_models, OllamaProvider};
pub use openai::OpenAiProvider;

/// Result type for LLM operations
pub type LlmResult<T> = Result<T, LlmError>;

/// Errors that can occur during LLM operations
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("API request failed: {0}")]
    ApiError(String),

    #[error("Request timed out after {0:?}")]
    Timeout(Duration),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Rate limit exceeded")]
    #[allow(dead_code)] // Reserved for future rate limit handling
    RateLimit,

    #[error("Response parsing failed: {0}")]
    ParseError(String),
}

/// Request to generate an AI answer
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    /// The prompt text
    pub prompt: String,
    /// Optional image URL for multimodal prompts
    pub image_url: Option<String>,
    /// Maximum response length in tokens (provider-dependent)
    pub max_tokens: Option<u32>,
    /// Timeout for the request
    pub timeout: Duration,
    /// Optional model override (e.g., "gpt-5" instead of configured model)
    pub model_override: Option<String>,
}

/// Response from an LLM provider
#[derive(Debug, Clone)]
pub struct GenerateResponse {
    /// The generated text
    pub text: String,
    /// Provider-specific metadata (model used, tokens consumed, etc.)
    pub metadata: ResponseMetadata,
}

/// Metadata about the LLM response
#[derive(Debug, Clone)]
pub struct ResponseMetadata {
    /// Name of the provider (e.g., "openai", "ollama")
    #[allow(dead_code)] // Used in tests and available for logging
    pub provider: String,
    /// Model name used
    pub model: String,
    /// Tokens consumed (if available)
    #[allow(dead_code)] // Available for cost tracking and monitoring
    pub tokens_used: Option<u32>,
    /// Latency in milliseconds
    pub latency_ms: u64,
}

/// Trait that all LLM providers must implement
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate an AI answer for the given prompt
    async fn generate(&self, request: GenerateRequest) -> LlmResult<GenerateResponse>;

    /// Get the name of this provider
    fn name(&self) -> &str;

    /// Whether this provider supports vision/image inputs
    fn supports_vision(&self) -> bool {
        false
    }
}

/// Manager for multiple LLM providers
pub struct LlmManager {
    pub providers: Vec<Box<dyn LlmProvider>>,
}

impl LlmManager {
    /// Create a new LLM manager with the given providers
    pub fn new(providers: Vec<Box<dyn LlmProvider>>) -> Self {
        Self { providers }
    }

    /// Generate answers from all available providers concurrently
    /// Returns (provider_name, response) pairs for successful generations
    /// For multimodal requests (with image_url), only vision-capable providers are used
    pub async fn generate_from_all(
        &self,
        request: GenerateRequest,
    ) -> Vec<(String, GenerateResponse)> {
        let mut tasks = Vec::new();
        let is_multimodal = request.image_url.is_some();

        for provider in &self.providers {
            // Skip providers that don't support vision for multimodal requests
            if is_multimodal && !provider.supports_vision() {
                tracing::info!(
                    "Skipping provider {} for multimodal request (no vision support)",
                    provider.name()
                );
                continue;
            }

            let req = request.clone();
            let provider_name = provider.name().to_string();
            let provider_ref = provider.as_ref();

            // Spawn concurrent generation tasks
            tasks.push(async move {
                match provider_ref.generate(req).await {
                    Ok(response) => Some((provider_name, response)),
                    Err(e) => {
                        tracing::error!("Provider {} failed: {}", provider_name, e);
                        None
                    }
                }
            });
        }

        if tasks.is_empty() && is_multimodal {
            tracing::warn!("No vision-capable providers available for multimodal request");
        }

        // Wait for all to complete and collect successes
        futures::future::join_all(tasks)
            .await
            .into_iter()
            .flatten()
            .collect()
    }

    /// Generate from a specific provider with optional model override
    /// model_id format: "provider:model" (e.g., "openai:gpt-5", "ollama:llama3.2")
    pub async fn generate_from_model(
        &self,
        model_id: &str,
        request: GenerateRequest,
    ) -> LlmResult<(String, GenerateResponse)> {
        let parts: Vec<&str> = model_id.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(LlmError::ConfigError(
                "Invalid model ID format, expected 'provider:model'".to_string(),
            ));
        }
        let (provider_name, model_name) = (parts[0], parts[1]);

        // Find provider by name
        let provider = self
            .providers
            .iter()
            .find(|p| p.name() == provider_name)
            .ok_or_else(|| {
                LlmError::ConfigError(format!("Provider '{}' not configured", provider_name))
            })?;

        // Check vision support for multimodal requests
        if request.image_url.is_some() && !provider.supports_vision() {
            return Err(LlmError::ConfigError(format!(
                "Provider '{}' does not support vision for multimodal requests",
                provider_name
            )));
        }

        // Create request with model override
        let request_with_override = GenerateRequest {
            model_override: Some(model_name.to_string()),
            ..request
        };

        let response = provider.generate(request_with_override).await?;
        Ok((provider_name.to_string(), response))
    }
}

/// Configuration for LLM providers
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// OpenAI API key
    pub openai_api_key: Option<String>,
    /// OpenAI model to use
    pub openai_model: String,
    /// Ollama base URL
    pub ollama_base_url: Option<String>,
    /// Ollama model to use
    pub ollama_model: String,
    /// Default timeout for LLM requests
    pub default_timeout: Duration,
    /// Default max tokens for responses
    pub default_max_tokens: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            openai_api_key: None,
            openai_model: "gpt-4o-mini".to_string(),
            ollama_base_url: Some("http://localhost:11434".to_string()),
            ollama_model: "llama3.2".to_string(),
            default_timeout: Duration::from_secs(30),
            default_max_tokens: 150,
        }
    }
}

impl LlmConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let openai_api_key = std::env::var("OPENAI_API_KEY").ok().and_then(|key| {
            let trimmed = key.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        });

        let openai_model = std::env::var("OPENAI_MODEL")
            .ok()
            .and_then(|model| {
                let trimmed = model.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| "gpt-4o-mini".to_string());

        let ollama_base_url = match std::env::var("OLLAMA_BASE_URL") {
            Ok(url) => {
                let trimmed = url.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            Err(_) => Some("http://localhost:11434".to_string()),
        };

        let ollama_model = std::env::var("OLLAMA_MODEL")
            .ok()
            .and_then(|model| {
                let trimmed = model.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| "llama3.2".to_string());

        Self {
            openai_api_key,
            openai_model,
            ollama_base_url,
            ollama_model,
            default_timeout: std::env::var("LLM_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .map(Duration::from_secs)
                .unwrap_or(Duration::from_secs(30)),
            default_max_tokens: std::env::var("LLM_MAX_TOKENS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(150),
        }
    }

    /// Build an LlmManager with all configured providers
    pub fn build_manager(&self) -> LlmResult<LlmManager> {
        let mut providers: Vec<Box<dyn LlmProvider>> = Vec::new();

        // Add OpenAI if API key is available
        if let Some(api_key) = &self.openai_api_key {
            providers.push(Box::new(OpenAiProvider::new(
                api_key.clone(),
                self.openai_model.clone(),
            )));
        }

        // Add Ollama if base URL is available
        if let Some(base_url) = &self.ollama_base_url {
            providers.push(Box::new(OllamaProvider::new(
                base_url.clone(),
                self.ollama_model.clone(),
            )));
        }

        if providers.is_empty() {
            return Err(LlmError::ConfigError(
                "No LLM providers configured. Set OPENAI_API_KEY or OLLAMA_BASE_URL".to_string(),
            ));
        }

        Ok(LlmManager::new(providers))
    }
}

/// System prompt for German typo correction
const TYPO_CORRECTION_SYSTEM_PROMPT: &str = r#"Du bist ein Rechtschreib- und Grammatikprüfer für deutsche Texte.

Korrigiere NUR:
- Rechtschreibfehler
- Groß-/Kleinschreibung
- Grammatikfehler
- Zeichensetzung

Ändere NICHTS anderes:
- Keine Umformulierungen
- Keine Stiländerungen
- Kein Hinzufügen oder Entfernen von Inhalten
- Behalte den ursprünglichen Ton bei

Gib NUR den korrigierten Text zurück, ohne Erklärungen oder Kommentare."#;

/// Check and correct typos in German text
/// Returns the corrected text, or the original if correction fails
pub async fn check_typos(provider: &dyn LlmProvider, text: &str) -> String {
    let request = GenerateRequest {
        prompt: text.to_string(),
        image_url: None,
        max_tokens: Some(500), // Allow for text that might slightly expand
        timeout: Duration::from_secs(5), // Short timeout for typo check
        model_override: None,  // Use default model for typo check
    };

    match check_typos_with_system_prompt(provider, request).await {
        Ok(response) => {
            let corrected = response.text.trim().to_string();
            // Sanity check: if the response is drastically different in length, use original
            // (LLM might have hallucinated)
            let len_ratio = corrected.len() as f64 / text.len() as f64;
            if !(0.5..=2.0).contains(&len_ratio) {
                tracing::warn!(
                    "Typo correction output length differs too much ({:.1}x), using original",
                    len_ratio
                );
                text.to_string()
            } else if corrected.is_empty() {
                tracing::warn!("Typo correction returned empty, using original");
                text.to_string()
            } else {
                corrected
            }
        }
        Err(e) => {
            tracing::warn!("Typo correction failed: {}, using original text", e);
            text.to_string()
        }
    }
}

/// Internal function to call LLM with typo correction system prompt
async fn check_typos_with_system_prompt(
    provider: &dyn LlmProvider,
    request: GenerateRequest,
) -> LlmResult<GenerateResponse> {
    // For OpenAI, we need to use a custom system prompt
    // Since the trait doesn't support system prompts directly,
    // we'll prepend instructions to the user prompt
    let modified_request = GenerateRequest {
        prompt: format!(
            "{}\n\nText zu korrigieren:\n{}",
            TYPO_CORRECTION_SYSTEM_PROMPT, request.prompt
        ),
        image_url: request.image_url,
        max_tokens: request.max_tokens,
        timeout: request.timeout,
        model_override: request.model_override,
    };

    provider.generate(modified_request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LlmConfig::default();
        assert_eq!(config.openai_model, "gpt-4o-mini");
        assert_eq!(config.ollama_model, "llama3.2");
        assert_eq!(config.default_timeout, Duration::from_secs(30));
    }
}
