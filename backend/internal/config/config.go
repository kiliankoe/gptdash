package config

import "os"

type Config struct {
	Port            string
	DefaultProvider string
	DefaultModel    string
	SystemPrompt    string
	OpenAIKey       string
	OpenAIBaseURL   string
	OllamaHost      string
	GMUser          string
	GMPass          string
	SingleSession   bool
}

func FromEnv() Config {
	c := Config{}
	c.Port = getenv("PORT", "8080")
	c.DefaultProvider = getenv("DEFAULT_PROVIDER", "openai")
	c.DefaultModel = getenv("DEFAULT_MODEL", "gpt-3.5-turbo")
	c.SystemPrompt = getenv("SYSTEM_PROMPT", "Du bist eine prägnante, sich kurzfassende KI. Antworte knapp in 1-2 Sätzen.")
	c.OpenAIKey = os.Getenv("OPENAI_API_KEY")
	c.OpenAIBaseURL = os.Getenv("OPENAI_BASE_URL")
	c.OllamaHost = getenv("OLLAMA_HOST", "http://localhost:11434")
	c.GMUser = os.Getenv("GM_USER")
	c.GMPass = os.Getenv("GM_PASS")
	c.SingleSession = getenv("SINGLE_SESSION", "true") == "true"
	return c
}

func getenv(k, def string) string {
	if v := os.Getenv(k); v != "" {
		return v
	}
	return def
}
