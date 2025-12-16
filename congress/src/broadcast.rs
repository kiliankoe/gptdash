use crate::protocol::ServerMessage;
use crate::state::AppState;
use crate::types::GamePhase;
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
