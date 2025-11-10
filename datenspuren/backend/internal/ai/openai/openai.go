package openai

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"
)

type Client struct {
	APIKey  string
	BaseURL string
	http    *http.Client
}

func New(apiKey, baseURL string) *Client {
	if baseURL == "" {
		baseURL = "https://api.openai.com"
	}
	return &Client{APIKey: apiKey, BaseURL: strings.TrimRight(baseURL, "/"), http: &http.Client{Timeout: 20 * time.Second}}
}

func (c *Client) Complete(ctx context.Context, model string, prompt string) (string, error) {
	return c.CompleteWithSystem(ctx, model, "", prompt)
}

func (c *Client) CompleteWithSystem(ctx context.Context, model string, systemPrompt string, prompt string) (string, error) {
	if c.APIKey == "" {
		return "", errors.New("missing OPENAI_API_KEY")
	}
	if systemPrompt == "" {
		systemPrompt = "Du bist eine prägnante, sich kurzfassende KI. Antworte knapp in 1-2 Sätzen."
	}
	if strings.Contains(model, "gpt") {
		return c.chatCompleteWithSystem(ctx, model, systemPrompt, prompt)
	}
	return c.textComplete(ctx, model, prompt)
}

func (c *Client) chatCompleteWithSystem(ctx context.Context, model string, systemPrompt string, prompt string) (string, error) {
	payload := map[string]any{
		"model": model,
		"messages": []map[string]string{
			{"role": "system", "content": systemPrompt},
			{"role": "user", "content": prompt},
		},
		"temperature": 0.8,
		"max_tokens":  200,
	}
	b, _ := json.Marshal(payload)
	req, _ := http.NewRequestWithContext(ctx, "POST", c.BaseURL+"/v1/chat/completions", bytes.NewReader(b))
	req.Header.Set("Authorization", "Bearer "+c.APIKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		return "", fmt.Errorf("openai status %d", resp.StatusCode)
	}
	var out struct {
		Choices []struct {
			Message struct {
				Content string `json:"content"`
			} `json:"message"`
		} `json:"choices"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return "", err
	}
	if len(out.Choices) == 0 {
		return "", errors.New("no choices")
	}
	return strings.TrimSpace(out.Choices[0].Message.Content), nil
}

func (c *Client) textComplete(ctx context.Context, model string, prompt string) (string, error) {
	payload := map[string]any{
		"model":       model,
		"prompt":      prompt,
		"temperature": 0.8,
		"max_tokens":  200,
	}
	b, _ := json.Marshal(payload)
	req, _ := http.NewRequestWithContext(ctx, "POST", c.BaseURL+"/v1/completions", bytes.NewReader(b))
	req.Header.Set("Authorization", "Bearer "+c.APIKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		return "", fmt.Errorf("openai status %d", resp.StatusCode)
	}
	var out struct {
		Choices []struct {
			Text string `json:"text"`
		} `json:"choices"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return "", err
	}
	if len(out.Choices) == 0 {
		return "", errors.New("no choices")
	}
	return strings.TrimSpace(out.Choices[0].Text), nil
}
