package ollama

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"
)

type Client struct {
	Host string
	http *http.Client
}

func New(host string) *Client {
	if host == "" {
		host = "http://localhost:11434"
	}
	return &Client{Host: strings.TrimRight(host, "/"), http: &http.Client{Timeout: 20 * time.Second}}
}

func (c *Client) Complete(ctx context.Context, model string, prompt string) (string, error) {
	return c.CompleteWithSystem(ctx, model, "", prompt)
}

func (c *Client) CompleteWithSystem(ctx context.Context, model string, systemPrompt string, prompt string) (string, error) {
	if systemPrompt == "" {
		systemPrompt = "You are a concise AI. Answer briefly in 1-2 sentences."
	}
	payload := map[string]any{
		"model": model,
		"messages": []map[string]string{
			{"role": "system", "content": systemPrompt},
			{"role": "user", "content": prompt},
		},
		"stream": false,
	}
	b, _ := json.Marshal(payload)
	req, _ := http.NewRequestWithContext(ctx, "POST", c.Host+"/api/chat", bytes.NewReader(b))
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		return "", fmt.Errorf("ollama status %d", resp.StatusCode)
	}
	var out struct {
		Message struct {
			Content string `json:"content"`
		} `json:"message"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return "", err
	}
	return strings.TrimSpace(out.Message.Content), nil
}
