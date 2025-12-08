use super::*;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Ollama provider implementation
pub struct OllamaProvider {
    base_url: String,
    model: String,
    client: reqwest::Client,
    /// Whether this model supports vision (e.g., llava, bakllava, moondream)
    supports_vision: bool,
}

impl OllamaProvider {
    /// Create a new Ollama provider with the given base URL and model
    pub fn new(base_url: String, model: String) -> Self {
        // Check if the model is a known vision model
        let supports_vision = Self::is_vision_model(&model);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap();

        Self {
            base_url,
            model,
            client,
            supports_vision,
        }
    }

    /// Check if a model name indicates vision support
    fn is_vision_model(model: &str) -> bool {
        let model_lower = model.to_lowercase();
        // Known vision models in Ollama
        model_lower.contains("llava")
            || model_lower.contains("bakllava")
            || model_lower.contains("moondream")
            || model_lower.contains("minicpm-v")
            || model_lower.contains("qwen2-vl")
            || model_lower.contains("qwen2.5-vl")
    }

    /// Fetch image from URL and encode as base64
    async fn fetch_image_as_base64(&self, url: &str) -> Result<String, LlmError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| LlmError::ApiError(format!("Failed to fetch image: {}", e)))?;

        if !response.status().is_success() {
            return Err(LlmError::ApiError(format!(
                "Failed to fetch image, status: {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| LlmError::ApiError(format!("Failed to read image bytes: {}", e)))?;

        use base64::{engine::general_purpose::STANDARD, Engine as _};
        Ok(STANDARD.encode(&bytes))
    }
}

#[derive(Debug, Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
    #[serde(default)]
    #[allow(dead_code)] // Part of Ollama API response format
    done: bool,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn generate(&self, request: GenerateRequest) -> LlmResult<GenerateResponse> {
        let start = Instant::now();

        // Build the prompt with game context (German)
        let system_prompt = "Bitte antworte in drei sehr kurzen Sätzen auf die folgende Frage oder Aufforderung. \
            Nur drei sehr kurze Sätze, keine Stichpunkte, bitte nur in Fließtext und nicht lang oder umschweifend. \
            Formuliere die kurzen Sätze bitte so wie ein Mensch, der die Antwort innerhalb von 2 Minuten selbst schreibt. \
            Vermeide komplexe Ausdrücke und Formulierungen. Einfach nur drei normale kurze Sätze.";

        let full_prompt = if request.image_url.is_some() {
            // For multimodal, adjust the prompt to reference the image
            if request.prompt.is_empty() {
                format!(
                    "{} Schau dir dieses Bild an und beschreibe es:",
                    system_prompt
                )
            } else {
                format!(
                    "{} Schau dir dieses Bild an. Die Frage lautet: {}",
                    system_prompt, request.prompt
                )
            }
        } else {
            format!("{} Die Frage lautet: {}", system_prompt, request.prompt)
        };

        // Handle image if present
        let images = if let Some(ref image_url) = request.image_url {
            if !self.supports_vision {
                return Err(LlmError::ConfigError(format!(
                    "Model {} does not support vision. Use a vision model like llava or moondream.",
                    self.model
                )));
            }

            // Ollama requires base64-encoded images
            let base64_image = self.fetch_image_as_base64(image_url).await?;
            Some(vec![base64_image])
        } else {
            None
        };

        let ollama_request = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt: full_prompt,
            stream: false,
            options: request.max_tokens.map(|num_predict| OllamaOptions {
                num_predict: Some(num_predict),
            }),
            images,
        };

        let url = format!("{}/api/generate", self.base_url);

        // Execute with timeout
        let response = tokio::time::timeout(
            request.timeout,
            self.client.post(&url).json(&ollama_request).send(),
        )
        .await
        .map_err(|_| LlmError::Timeout(request.timeout))?
        .map_err(|e| LlmError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(LlmError::ApiError(format!(
                "Ollama API returned status: {}",
                response.status()
            )));
        }

        let ollama_response: OllamaGenerateResponse = response
            .json()
            .await
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        let latency_ms = start.elapsed().as_millis() as u64;

        Ok(GenerateResponse {
            text: ollama_response.response.trim().to_string(),
            metadata: ResponseMetadata {
                provider: "ollama".to_string(),
                model: self.model.clone(),
                tokens_used: None, // Ollama doesn't return token counts in this API
                latency_ms,
            },
        })
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn supports_vision(&self) -> bool {
        self.supports_vision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Only run with Ollama running locally
    async fn test_ollama_generate() {
        let provider =
            OllamaProvider::new("http://localhost:11434".to_string(), "llama3.2".to_string());

        let request = GenerateRequest {
            prompt: "What's the best way to make a sandwich?".to_string(),
            image_url: None,
            max_tokens: Some(100),
            timeout: Duration::from_secs(30),
        };

        let response = provider.generate(request).await.unwrap();

        assert!(!response.text.is_empty());
        assert_eq!(response.metadata.provider, "ollama");
        assert!(response.metadata.latency_ms > 0);
        println!("Generated text: {}", response.text);
        println!("Metadata: {:?}", response.metadata);
    }
}
