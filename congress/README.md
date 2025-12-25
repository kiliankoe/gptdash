# GPTDash

A Balderdash-style party game where players try to impersonate an AI. Audience guesses which answer is the real AI. Host drives the show; a "Beamer" screen shows the stage visuals.

## Quick Start

**Prerequisites**: Rust toolchain

```bash
# Run the server
cargo run

# Access the interfaces
# Audience:  http://localhost:6573/
# Beamer:    http://localhost:6573/beamer
# Player:    http://localhost:6573/player
# Host:      http://localhost:6573/host
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `BIND_ADDR` | 0.0.0.0 | Server bind address |
| `PORT` | 6573 | Server port |
| `HOST_USERNAME` | (none) | Host panel basic auth username |
| `HOST_PASSWORD` | (none) | Host panel basic auth password |
| `OPENAI_API_KEY` | (none) | OpenAI API key |
| `OPENAI_MODEL` | gpt-4o-mini | OpenAI model |
| `OLLAMA_BASE_URL` | http://localhost:11434 | Ollama server URL |
| `OLLAMA_MODEL` | llama3.2 | Ollama model |
| `LLM_TIMEOUT` | 30 | LLM request timeout (seconds) |
| `LLM_MAX_TOKENS` | 150 | Max tokens for LLM responses |
| `AUTO_SAVE_PATH` | ./state_backup.json | Auto-save file path |
| `AUTO_SAVE_INTERVAL_SECS` | 5 | Auto-save interval (seconds) |
| `DISABLE_AUTO_SAVE` | (unset) | Set to 1 to disable auto-save |
| `VENUE_IP_RANGES` | (none) | Comma-separated CIDR ranges for venue-only mode |
| `ABUSE_BLOCK_USER_AGENTS` | true | Block suspicious user agents |
| `ABUSE_REQUIRE_BROWSER` | true | Require browser headers for WebSocket |
| `ABUSE_RATE_LIMIT` | true | Enable rate limiting |
| `ABUSE_RATE_LIMIT_MAX` | 100 | Max requests per rate limit window |
| `ABUSE_RATE_LIMIT_WINDOW` | 10 | Rate limit window (seconds) |
| `SKIP_VOTE_ANTI_AUTOMATION` | (unset) | Set to 1 to disable vote anti-automation (testing) |
| `RUST_LOG` | gptdash=debug,tower_http=debug | Tracing log level |

## Game Concept

- Host presents up to 3 prompts; audience votes one in (or host selects directly)
- Players (on stage) and an LLM each submit short 2-3 sentence answers
- Answers are revealed one-by-one on the Beamer (with text-to-speech)
- Audience votes for: (1) the answer they believe is the AI, and (2) the funniest answer
- Points awarded to players who attract votes; audience members who correctly identify the AI also score
- Two leaderboards: Players and Audience

The show may run multiple rounds, then reset for a new set of volunteers.

## Roles

- **Host**: Single controller of game progression and curation tools (reorder answers, typo fixes, timers, panic mode)
- **Beamer**: Read-only stage display (scenes, countdowns, reveal carousel, live vote bars)
- **Player**: Can submit one answer per round from their device
- **Audience**: Can join anytime (QR/URL), propose prompts, vote in prompt selection and round voting

## Game Phases

| Phase | Description |
|-------|-------------|
| `LOBBY` | QR/URL visible, audience can join and submit prompt suggestions |
| `PROMPT_SELECTION` | Show up to 3 prompts; audience votes OR host picks directly |
| `WRITING` | Players write answers; LLM generates answer; host sees live submissions |
| `REVEAL` | Answers presented one-by-one with TTS |
| `VOTING` | Audience casts two votes; Beamer shows live animated bars |
| `RESULTS` | Reveal AI, funniest answer, update leaderboards |
| `PODIUM` | Winner's podium with top 3 players and audience AI detectors |
| `INTERMISSION` | Manual pause/panic or between rounds |

## Round Lifecycle

### 0. Setup
- Host creates players (generates player tokens)
- Players join from their devices and choose a display name
- Audience may submit prompt suggestions
- Host curates the prompt pool and queues up to 3 prompts

### 1. Prompt Selection (~10s)
- If 2-3 prompts queued: show candidates, audience votes
- If 1 prompt queued: skip directly to Writing
- Host may override and directly select a prompt
- On selection: start LLM generation, start writing countdown

### 2. Writing (~60-90s)
- Players can submit and resubmit (latest wins)
- Identical submissions blocked; host can flag similar ones
- Host can edit display text (typo fixes)
- Host can reorder submissions for reveal
- Timer is visual only; host advances manually

### 3. Reveal (sequential)
- Beamer shows each answer one at a time
- Browser TTS reads each answer aloud
- Host advances manually (next/prev)

### 4. Voting (~15-30s)
- Audience devices show ballot UI (no live counts on devices)
- Beamer shows animated vote bars (2 Hz updates)
- Audience may change choices until host closes voting

### 5. Results (~30s)
- Reveal which submission was AI
- Show funniest submission
- Display player and audience leaderboards
- Host can proceed to Podium, next round, or Intermission

### 6. Podium
- Display winner's podium: top 3 players, top 3 audience AI detectors
- Audience winners see green screen on their device for prize verification
- Time to hand out prizes and invite new volunteers

### 7. Next Round / Reset
- Increment round number, create new round
- Or: clear player roster for new volunteers (keeps audience)
- Or: full reset for fresh show

## Scoring

**Players**:
- +1 point per audience AI vote received (their answer fooled voters)
- +1 point per funny vote received

**Audience**:
- +1 point if their AI pick matches the actual AI submission

Tie-breakers: Higher AI-detect points, then earliest correct vote timestamp.

## Client Views

- **Host** (`/host`): Desktop control panel with sidebar navigation, real-time status, full game control, panic mode, state export/import. Protected by HTTP Basic Auth.
- **Beamer** (`/beamer`): Full-screen stage display with scenes for all phases, vote bars, reveal carousel with TTS, timer countdown, leaderboards. Protected by HTTP Basic Auth (same credentials as Host).
- **Player** (`/player`): Mobile interface for token entry, name registration, answer submission with character counter.
- **Audience** (`/`): Mobile voting interface with two-category ballot, prompt submission, trivia voting.

## Operational Notes

### Transport
- Native WebSockets via Axum
- Keep payloads small; text length is clamped
- Consider WebSocket compression at proxy if needed

### Scaling
- 3k audience @ 2 Hz Beamer updates is well within one machine
- Compute aggregates once per tick; reuse payload

### Timing
- Timers are server-authored deadlines
- Clients display deltas based on server time
- Host advances phases manually (timers are visual only)

### LLM/TTS
- LLM generation kicks off immediately when prompt is chosen
- Multiple LLM backends run concurrently; host picks best answer
- Browser TTS API used for answer reveals (no pre-rendering)

### Reliability
- Single-writer guard for host actions
- Idempotent vote writes with `msg_id` deduplication
- Auto-save every 5 seconds; auto-restore on startup
- Panic mode available to disable audience interactions

## Anti-Abuse

- **Rate limiting**: Per-token rate limiting on WebSocket connections
- **User-Agent blocking**: Blocks curl, wget, bots
- **Browser header validation**: Requires `Sec-WebSocket-Key` and `Origin` headers
- **Vote challenge**: SHA256 challenge-response to block trivial vote scripting
- **Anti-automation**: Server-side timing + automation detection (webdriver, PhantomJS, Nightmare, Cypress) with shadow rejection
- **Shadowban**: Host can silently ignore spammy audience members
- **Venue-only mode**: Restrict audience to specific IP ranges (see below)

## Venue-Only Mode

Restricts audience membership to people physically at the venue by IP address filtering. Players, host, and beamer are exempt.

**Configuration:**
- Set IP ranges via `VENUE_IP_RANGES` env var (comma-separated CIDR notation, e.g., `185.1.74.0/24,2001:db8::/32`)
- Host toggles venue mode on/off via host panel

**Behavior:**
- Blocks both HTTP pages and WebSocket connections for non-venue audience
- Supports X-Forwarded-For header for reverse proxy deployments
- Empty ranges = allow all (safety default to prevent accidental lockout)
- Enabled state persists in state export/import

## Future Work

- **Postgres persistence**: Event sourcing with snapshots for true durability
- **Metrics**: WebSocket connections, vote rate, latency tracking
- **Deployment config**: TLS setup, production hardening

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed technical documentation including:
- Project structure and module responsibilities
- State management and state machine validation
- Security implementation details
- Testing strategy
