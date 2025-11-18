mod ollama;
mod openai;

use async_trait::async_trait;
use std::time::Duration;

pub use ollama::OllamaProvider;
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
}

/// Manager for multiple LLM providers
pub struct LlmManager {
    providers: Vec<Box<dyn LlmProvider>>,
}

impl LlmManager {
    /// Create a new LLM manager with the given providers
    pub fn new(providers: Vec<Box<dyn LlmProvider>>) -> Self {
        Self { providers }
    }

    /// Generate answers from all available providers concurrently
    /// Returns (provider_name, response) pairs for successful generations
    pub async fn generate_from_all(
        &self,
        request: GenerateRequest,
    ) -> Vec<(String, GenerateResponse)> {
        let mut tasks = Vec::new();

        for provider in &self.providers {
            let req = request.clone();
            let provider_name = provider.name().to_string();
            let provider_ref = provider.as_ref();

            // Spawn concurrent generation tasks
            tasks.push(async move {
                match provider_ref.generate(req).await {
                    Ok(response) => Some((provider_name, response)),
                    Err(e) => {
                        eprintln!("Provider {} failed: {}", provider_name, e);
                        None
                    }
                }
            });
        }

        // Wait for all to complete and collect successes
        futures::future::join_all(tasks)
            .await
            .into_iter()
            .flatten()
            .collect()
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
        Self {
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            openai_model: std::env::var("OPENAI_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            ollama_base_url: std::env::var("OLLAMA_BASE_URL")
                .ok()
                .or_else(|| Some("http://localhost:11434".to_string())),
            ollama_model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string()),
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
