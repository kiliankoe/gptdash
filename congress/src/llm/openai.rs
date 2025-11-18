use super::*;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
    Client,
};
use std::time::Instant;

/// OpenAI provider implementation
pub struct OpenAiProvider {
    client: Client<OpenAIConfig>,
    model: String,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider with the given API key and model
    pub fn new(api_key: String, model: String) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);

        Self { client, model }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn generate(&self, request: GenerateRequest) -> LlmResult<GenerateResponse> {
        let start = Instant::now();

        // Build system prompt for the game
        let system_content = "You are playing a party game where you impersonate a human player. \
            The host will give you a prompt, and you must provide a witty, creative, and entertaining \
            2-3 sentence answer that sounds like it could have been written by a real person. \
            Be funny, clever, and slightly irreverent. Don't be too formal or robotic. \
            Keep your answer concise and entertaining.";

        // Build user prompt
        let user_content = format!(
            "Prompt: {}\n\nProvide your 2-3 sentence answer:",
            request.prompt
        );

        // TODO: Handle image_url for multimodal prompts when needed
        if request.image_url.is_some() {
            return Err(LlmError::ConfigError(
                "Multimodal prompts not yet implemented for OpenAI".to_string(),
            ));
        }

        // Create the chat completion request
        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder.model(&self.model).messages([
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_content)
                .build()
                .map_err(|e| LlmError::ApiError(e.to_string()))?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_content)
                .build()
                .map_err(|e| LlmError::ApiError(e.to_string()))?
                .into(),
        ]);

        // Set max tokens if provided
        if let Some(max_tokens) = request.max_tokens {
            req_builder.max_tokens(max_tokens);
        }

        let chat_request = req_builder
            .build()
            .map_err(|e| LlmError::ApiError(e.to_string()))?;

        // Execute with timeout
        let response =
            tokio::time::timeout(request.timeout, self.client.chat().create(chat_request))
                .await
                .map_err(|_| LlmError::Timeout(request.timeout))?
                .map_err(|e| LlmError::ApiError(e.to_string()))?;

        // Extract the generated text
        let text = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| LlmError::ParseError("No content in response".to_string()))?;

        let latency_ms = start.elapsed().as_millis() as u64;

        // Calculate tokens used
        let tokens_used = response.usage.map(|u| u.total_tokens);

        Ok(GenerateResponse {
            text: text.trim().to_string(),
            metadata: ResponseMetadata {
                provider: "openai".to_string(),
                model: self.model.clone(),
                tokens_used,
                latency_ms,
            },
        })
    }

    fn name(&self) -> &str {
        "openai"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Only run with actual API key
    async fn test_openai_generate() {
        let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
        let provider = OpenAiProvider::new(api_key, "gpt-4o-mini".to_string());

        let request = GenerateRequest {
            prompt: "What's the best way to make a sandwich?".to_string(),
            image_url: None,
            max_tokens: Some(100),
            timeout: Duration::from_secs(30),
        };

        let response = provider.generate(request).await.unwrap();

        assert!(!response.text.is_empty());
        assert_eq!(response.metadata.provider, "openai");
        assert!(response.metadata.latency_ms > 0);
        println!("Generated text: {}", response.text);
        println!("Metadata: {:?}", response.metadata);
    }
}
