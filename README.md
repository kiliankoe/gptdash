# GPTdash

A real-time party game based loosely on [Balderdash](https://en.wikipedia.org/wiki/Balderdash), but with AI. Players answer questions while trying to impersonate the AI, then vote to identify the real AI answer. Score points by fooling others or correctly identifying the AI!

Originally built for [Datenspuren 2024](https://www.datenspuren.de/2024/).

Sorry, the German is currently only in German. PRs welcome 😁

## Quick Start

The game is available as a pre-built binary containing backend and frontend or as a docker image. Pick whatever works best for you. Or, you know, just build it from source.

Either OpenAI or Ollama are supported as AI providers. For OpenAI, it is suggested to use gpt-3.5-turbo or older. Newer models make the game less fun.

### Using Docker
```bash
docker run -p 8080:8080 -e OPENAI_API_KEY=your-key ghcr.io/kiliankoe/gptdash:latest
```

### Using the binary
1. Download the latest release for your platform
2. Set environment variables (see `.env.example`)
3. Run `./gptdash`

Visit http://localhost:8080 to play!

## Building from Source

### Prerequisites
- Node.js 24+
- Go 1.24+
- Make

### Build
```bash
# Clone the repository
git clone https://github.com/kiliankoe/gptdash
cd gptdash

# Copy environment variables
cp .env.example .env
# Edit .env and add your OPENAI_API_KEY or set up Ollama

# Build everything
make build

# Run
./gptdash
```

### Development
```bash
# Frontend development server (hot reload)
cd frontend && npm run dev

# Backend development (requires built frontend)
cd backend && go run ./cmd/server
```

## Configuration

Key environment variables:
- `OPENAI_API_KEY` - Required for OpenAI provider
- `DEFAULT_MODEL` - AI model to use (default: gpt-3.5-turbo)
- `EXPORT_ENABLED` - Save game results to file (default: true)
- `GM_USER`/`GM_PASS` - Optional GM interface authentication

See `.env.example` for all options.
