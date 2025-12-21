use super::*;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessageContentPartImage, ChatCompletionRequestMessageContentPartText,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, ChatCompletionRequestUserMessageContentPart,
        CreateChatCompletionRequestArgs, ImageDetail, ImageUrl,
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

        // Build system prompt for the game (German)
        let system_content = "Bitte antworte in drei sehr kurzen Sätzen auf die folgende Frage oder Aufforderung. \
            Nur drei sehr kurze Sätze, keine Stichpunkte, bitte nur in Fließtext und nicht lang oder umschweifend. \
            Formuliere die kurzen Sätze bitte so wie ein Mensch, der die Antwort innerhalb von 2 Minuten selbst schreibt. \
            Vermeide komplexe Ausdrücke und Formulierungen. Einfach nur drei normale kurze Sätze. Die Frage lautet:";

        // Build user message - either text-only or multimodal with image
        let user_message: ChatCompletionRequestUserMessage = if let Some(ref image_url) =
            request.image_url
        {
            // Multimodal request with image
            let mut content_parts: Vec<ChatCompletionRequestUserMessageContentPart> = Vec::new();

            // Add image part
            content_parts.push(ChatCompletionRequestUserMessageContentPart::ImageUrl(
                ChatCompletionRequestMessageContentPartImage {
                    image_url: ImageUrl {
                        url: image_url.clone(),
                        detail: Some(ImageDetail::Auto),
                    },
                },
            ));

            // Add text prompt if present
            let text_prompt = if request.prompt.is_empty() {
                "Look at this image. Provide your 2-3 sentence answer as if you were a human player:".to_string()
            } else {
                format!(
                    "Look at this image. Prompt: {}\n\nProvide your 2-3 sentence answer:",
                    request.prompt
                )
            };
            content_parts.push(ChatCompletionRequestUserMessageContentPart::Text(
                ChatCompletionRequestMessageContentPartText { text: text_prompt },
            ));

            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Array(content_parts),
                name: None,
            }
        } else {
            // Text-only request
            let user_content = format!(
                "Prompt: {}\n\nProvide your 2-3 sentence answer:",
                request.prompt
            );

            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(user_content),
                name: None,
            }
        };

        // Use model override if provided, otherwise use configured model
        let model = request
            .model_override
            .clone()
            .unwrap_or_else(|| self.model.clone());

        // Create the chat completion request
        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder.model(&model).messages([
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_content)
                .build()
                .map_err(|e| LlmError::ApiError(e.to_string()))?
                .into(),
            user_message.into(),
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
                model, // Use the actual model used (may be override)
                tokens_used,
                latency_ms,
            },
        })
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn supports_vision(&self) -> bool {
        // Most modern OpenAI models support vision (gpt-4o, gpt-4o-mini, gpt-5, gpt-5.1, etc.)
        // Models that don't support vision will return an API error
        true
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
            model_override: None,
        };

        let response = provider.generate(request).await.unwrap();

        assert!(!response.text.is_empty());
        assert_eq!(response.metadata.provider, "openai");
        assert!(response.metadata.latency_ms > 0);
        println!("Generated text: {}", response.text);
        println!("Metadata: {:?}", response.metadata);
    }
}
