# GPTDash Architecture

## Module Responsibilities

### `state/`
Manages all in-memory game state with `Arc<RwLock<>>` for thread-safe async access.

- **mod.rs**: AppState struct with 70+ fields, broadcast channels, core state operations
- **game.rs**: Game lifecycle, phase transitions with validation, panic mode, deadlines
- **player.rs**: Player tokens, registration, lookup, mid-round removal with cascade cleanup
- **round.rs**: Round creation, prompt selection, reveal order, LLM generation triggers
- **submission.rs**: Submit/edit answers, duplicate detection, retrieval
- **vote.rs**: Vote recording with idempotency (msg_id), aggregation, anti-automation checks
- **score.rs**: Scoring computation (AI detect +1, funny +1), leaderboards
- **trivia.rs**: Trivia CRUD, presentation, voting, result computation
- **export.rs**: State serialization for backup/restore

### `llm/`
LLM provider abstraction for generating AI answers.

- **mod.rs**: `LlmProvider` trait, `LlmManager` for concurrent providers, config from env
- **openai.rs**: OpenAI implementation with vision support
- **ollama.rs**: Ollama implementation with base64 image handling

### `ws/`
WebSocket connection and message handling by role.

- **mod.rs**: Connection upgrade, lifecycle, broadcast subscriptions, state recovery
- **handlers.rs**: Message dispatch with `check_host!` authorization macro
- **host.rs**: 50+ host command handlers (player/prompt/submission/phase management)
- **player.rs**: Registration, answer submission, typo checking
- **audience.rs**: Voting (with anti-automation), prompt submission (with shadowban)

## Key Design Decisions

### State Management
All state uses `Arc<RwLock<>>` for thread-safe async access. The `AppState` struct contains 70+ fields covering game state, broadcast channels, and runtime config. Methods are spread across `state/` modules but operate on the same struct via `impl AppState` blocks.

### State Machine Validation
Both `GamePhase` and `RoundState` transitions are fully validated with preconditions:

**GamePhase flow**: Lobby → PromptSelection → Writing → Reveal → Voting → Results → Podium
- Special flows: any phase → Intermission (panic) or Ended (hard stop)
- Preconditions enforced (e.g., Writing requires selected prompt, Reveal requires submissions)

**RoundState flow**: Setup → Collecting → Revealing → OpenForVotes → Scoring → Closed
- Linear progression with precondition checks at each transition

### Global Prompt Pool
Prompts persist in a global pool across rounds and game resets. This allows audience to submit prompts throughout the evening, host to curate a backlog, and unused prompts to carry over. Game reset preserves the pool; host can explicitly clear it.

### Timer System
Timers are visual aids, not hard locks. Server sets `phase_deadline` when entering Writing/Voting phases. Clients display server-synchronized countdowns. Host advances phases manually and can extend timers via `HostExtendTimer`.

### LLM Integration
Multiple providers run concurrently when a prompt is selected. Host sees all AI responses with provider metadata and selects the best one via `HostSetAiSubmission`. Supports vision for multimodal prompts (image URLs). Graceful degradation if no providers configured.

### Multimodal Prompts
Prompts can include `image_url` alongside text. Images displayed on beamer/player screens and passed to vision-capable LLMs (OpenAI gpt-4o+, Ollama llava/moondream models).

### Audience Naming
Audience members get auto-generated friendly names on first connect (e.g., "Happy Hippo") using the petname crate. Names persist across reconnects and appear on leaderboards.

## Client Views

All views are static HTML/CSS/JS served from `static/` with WebSocket auto-reconnect.

- **Beamer** (`/beamer`): Full-screen stage display with scenes for all phases, vote bars (2 Hz updates), reveal carousel with TTS, timer countdown, leaderboards. Auth protected.
- **Player** (`/player`): Mobile interface for token entry, name registration, answer submission with character counter, typo correction flow
- **Audience** (`/`): Mobile voting interface with two-category ballot, prompt submission, trivia voting, top-3 winner green screen in Podium phase
- **Host** (`/host`): Desktop control panel with sidebar navigation, real-time status, full game control, panic mode, state export/import. Auth protected.

## Frontend Architecture

### CSS Structure

Two design systems coexist:

**39C3 Design System** (audience, beamer):
- `fonts.css` - 39C3 font faces (Kario, OfficerSans)
- `variables.css` - Color palette, typography variables, medal colors
- `audience.css` / `beamer.css` - Import shared files via `@import`

**Gradient Design System** (player, host):
- `common.css` - Reset, buttons, forms, cards, utility classes
- `player.css` / `host.css` - Extend common.css

**Utility Classes** (in common.css):
- Spacing: `.mt-10`, `.mb-15`, `.my-10`
- Typography: `.text-secondary`, `.text-muted`, `.form-label`
- Layout: `.flex-row`, `.gap-10`, `.w-full`

**Accessibility**:
- `:focus-visible` styles for keyboard navigation
- `@media (prefers-reduced-motion)` in all CSS files
- Semantic HTML: `<nav>` for navigation, `<button>` for interactive elements

**JavaScript**: Shared `common.js` provides:
- `WSConnection`: Auto-reconnecting WebSocket with 2s delay
- `CountdownTimer`: Server-synchronized countdown with deadline tracking
- `TTSManager`: Browser SpeechSynthesis integration
- Utilities: `showScreen()`, `escapeHtml()`, `copyToClipboard()`, etc.

View-specific files handle UI logic. Host panel is further modularized into `js/host/` with separate modules for each panel (overview, players, prompts, submissions, AI manager, state export).

## Security

### Authentication
Host panel and beamer display protected by HTTP Basic Auth via `HOST_USERNAME`/`HOST_PASSWORD` env vars. Credentials validated with constant-time comparison. Both HTTP routes and WebSocket connections (`role=host`, `role=beamer`) are protected.

### Anti-Abuse (src/abuse.rs)
Applied to `/ws` route only:
- **User-Agent blocking**: Blocks curl, wget, bots, empty UA
- **Browser header validation**: Requires `Sec-WebSocket-Key` and `Origin` headers
- **Rate limiting**: Per-token, configurable via `ABUSE_RATE_LIMIT_*` env vars

### Vote Security
Layered defense against vote manipulation:
1. **Challenge-response**: Server sends nonce on VOTING start; client computes `SHA256(nonce + voter_token)[0:16]`
2. **Server-side timing**: Votes within 500ms of phase start are silently discarded
3. **Automation detection**: Votes from automated browsers silently discarded (detects `navigator.webdriver`, `window._phantom`, `window.__nightmare`, `window.Cypress`)
4. **Shadow rejection**: Suspicious votes get `VoteAck` but aren't stored (no feedback to attacker)

Disabled via `SKIP_VOTE_ANTI_AUTOMATION=1` for testing.

### Moderation
Host can shadowban audience members via `HostShadowbanAudience`. Shadowbanned users' prompt submissions are silently ignored (they see success). Shadowban list included in state exports.

## Persistence

### State Export/Import
- `GET /api/state/export`: Returns full game state as JSON
- `POST /api/state/import`: Replaces state with uploaded JSON

Both endpoints protected by host auth. Exports include schema version for compatibility. Post-import broadcasts `GameState` to all clients.

### Auto-Save/Load
Background task saves state to `AUTO_SAVE_PATH` (default: `./state_backup.json`) every `AUTO_SAVE_INTERVAL_SECS` (default: 5). On startup, restores from backup if found. Disabled via `DISABLE_AUTO_SAVE=1` for testing.

### What's Exported
Game, rounds, submissions, votes, players, scores, prompt pool, audience members, shadowban list, trivia questions, player status, vote deduplication state.

**Not exported**: Broadcast channels, LlmManager, API keys.

## Special Features

### Panic Mode
Emergency toggle that disables all audience voting. Server rejects votes with `PANIC_MODE` error. Audience devices show "vote by clapping" overlay. Host can manually select winners via `HostSetManualWinner`.

### Trivia System
Entertainment during WRITING phase. Host adds 2-4 choice questions to pool, presents one, audience votes (hidden), host resolves to show results. No scoring, purely entertainment. Auto-clears on phase change. Persists in state exports.

### Typo Correction
After player submits, background LLM check offers corrections. Player can accept (updates submission) or reject (keeps original). Non-blocking with 5s timeout. German-focused system prompt.

### Dynamic Player Management
Host can add players anytime via `HostCreatePlayers`. Removing a player mid-round (`HostRemovePlayer`) triggers cascade cleanup: removes submissions, updates reveal order, clears affected votes (allowing re-vote).

### Reconnection & State Recovery
Players and audience reconnect with their tokens via query params. Server sends phase-appropriate state:
- Players: `PlayerState` with submission status
- Audience: `AudienceState` with current vote, `VoteChallenge` if in VOTING
- Beamer: Current prompt, submissions, vote counts, scores as appropriate

Vote idempotency via `msg_id` per voter ensures reliable delivery.

## Testing

### Unit Tests
Inline `#[cfg(test)]` blocks in state modules covering:
- State transitions and validation
- Authorization checks
- Vote aggregation and idempotency
- Score computation

### Integration Tests (`tests/integration_test.rs`)
~2800 lines covering full game flows, error handling, multi-role interactions.

### E2E Tests (`e2e/`)
11 Playwright test suites covering all game flows, reconnection, state restoration.

### Running Tests
```bash
cargo test                                    # Unit + integration
cargo fmt && cargo clippy                     # Formatting + lints
npx playwright test                           # E2E (requires server)
biome lint static/ && biome format static/    # Frontend lints
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST_USERNAME` | (none) | Host panel basic auth username |
| `HOST_PASSWORD` | (none) | Host panel basic auth password |
| `OPENAI_API_KEY` | (none) | OpenAI API key |
| `OPENAI_MODEL` | gpt-4o-mini | OpenAI model |
| `OLLAMA_BASE_URL` | http://localhost:11434 | Ollama server URL |
| `OLLAMA_MODEL` | llama3.2 | Ollama model |
| `LLM_TIMEOUT` | 30 | LLM request timeout (seconds) |
| `LLM_MAX_TOKENS` | 150 | Max tokens for LLM responses |
| `AUTO_SAVE_PATH` | ./state_backup.json | Auto-save file path |
| `AUTO_SAVE_INTERVAL_SECS` | 5 | Auto-save interval |
| `DISABLE_AUTO_SAVE` | (unset) | Set to 1 to disable auto-save |
| `SKIP_VOTE_ANTI_AUTOMATION` | (unset) | Set to 1 for testing |
| `ABUSE_BLOCK_USER_AGENTS` | true | Block suspicious user agents |
| `ABUSE_REQUIRE_BROWSER` | true | Require browser headers |
| `ABUSE_RATE_LIMIT` | true | Enable rate limiting |
| `ABUSE_RATE_LIMIT_MAX` | 100 | Max requests per window |
| `ABUSE_RATE_LIMIT_WINDOW` | 10 | Window duration (seconds) |
