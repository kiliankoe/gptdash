package ai

import "context"

type Provider interface {
    Complete(ctx context.Context, model string, prompt string) (string, error)
    CompleteWithSystem(ctx context.Context, model string, systemPrompt string, prompt string) (string, error)
}

type Config struct {
    DefaultProvider string
    DefaultModel    string
    SystemPrompt    string
    OpenAIKey       string
    OpenAIBaseURL   string
    OllamaHost      string
}

