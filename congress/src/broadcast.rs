use crate::abuse::AbuseConfig;
use crate::protocol::ServerMessage;
use crate::state::export::GameStateExport;
use crate::state::AppState;
use crate::types::GamePhase;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Spawn a background task that broadcasts vote counts to Beamer clients during VOTING phase
pub fn spawn_vote_broadcaster(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut seq = 0u64;

        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Check if we're in VOTING phase
            let game = match state.get_game().await {
                Some(g) => g,
                None => continue,
            };

            if game.phase != GamePhase::Voting {
                continue;
            }

            // Get current round
            let round = match state.get_current_round().await {
                Some(r) => r,
                None => continue,
            };

            // Aggregate votes
            let (ai_counts, funny_counts) = state.aggregate_votes(&round.id).await;

            // Broadcast to Beamer clients
            seq += 1;
            let msg = ServerMessage::BeamerVoteCounts {
                ai: ai_counts,
                funny: funny_counts,
                seq,
            };

            // Ignore send errors (no receivers connected is fine)
            let _ = state.beamer_broadcast.send(msg);
        }
    });
}

/// Spawn a background task that broadcasts prompt vote counts to Beamer clients during PROMPT_SELECTION phase
pub fn spawn_prompt_vote_broadcaster(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Check if we're in PROMPT_SELECTION phase
            let game = match state.get_game().await {
                Some(g) => g,
                None => continue,
            };

            if game.phase != GamePhase::PromptSelection {
                continue;
            }

            // Get prompt vote counts
            let counts = state.get_prompt_vote_counts().await;

            // Broadcast to Beamer clients
            let msg = ServerMessage::BeamerPromptVoteCounts { counts };

            // Ignore send errors (no receivers connected is fine)
            let _ = state.beamer_broadcast.send(msg);
        }
    });
}

/// Spawn a background task that broadcasts connection stats to Host clients every second
pub fn spawn_connection_stats_broadcaster(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let counts = state.get_connection_counts();
            let msg = ServerMessage::HostConnectionStats {
                players: counts.players,
                audience: counts.audience,
                beamers: counts.beamers,
                hosts: counts.hosts,
            };

            // Ignore send errors (no receivers connected is fine)
            let _ = state.host_broadcast.send(msg);
        }
    });
}

/// Save current state to a file (extracted for testability)
pub async fn save_state_to_file(state: &AppState, path: &Path) -> Result<(), String> {
    let export = state.export_state().await;
    let json =
        serde_json::to_string_pretty(&export).map_err(|e| format!("Failed to serialize: {}", e))?;
    tokio::fs::write(path, &json)
        .await
        .map_err(|e| format!("Failed to write: {}", e))?;
    Ok(())
}

/// Load state from a file (extracted for testability)
pub async fn load_state_from_file(state: &AppState, path: &Path) -> Result<(), String> {
    let json = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("Failed to read: {}", e))?;
    let export: GameStateExport =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse: {}", e))?;
    state.import_state(export).await
}

/// Spawn background task that periodically saves state to disk
pub fn spawn_auto_save_task(state: Arc<AppState>) {
    // Check if disabled (for tests)
    let disable_auto_save = std::env::var("DISABLE_AUTO_SAVE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    if disable_auto_save {
        tracing::info!("Auto-save disabled via DISABLE_AUTO_SAVE env var");
        return;
    }

    let interval_secs: u64 = std::env::var("AUTO_SAVE_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5); // Default: every 5 seconds

    let save_path =
        std::env::var("AUTO_SAVE_PATH").unwrap_or_else(|_| "./state_backup.json".to_string());
    let save_path = PathBuf::from(save_path);

    tracing::info!(
        "Auto-save enabled: saving to {:?} every {}s",
        save_path,
        interval_secs
    );

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;

            if let Err(e) = save_state_to_file(&state, &save_path).await {
                tracing::error!("Auto-save failed: {}", e);
            }
        }
    });
}

/// Spawn background task that periodically cleans up stale rate limiter entries
/// This prevents memory leaks from accumulated token entries
pub fn spawn_rate_limiter_cleanup(abuse_config: Arc<AbuseConfig>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;

            if let Some(ref limiter) = abuse_config.rate_limiter {
                limiter.cleanup().await;
            }
        }
    });
}
