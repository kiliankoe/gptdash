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

/// Spawn a background task that auto-advances from VOTING to RESULTS when deadline expires
pub fn spawn_voting_deadline_watcher(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Check if we're in VOTING phase with a deadline
            let game = match state.get_game().await {
                Some(g) => g,
                None => continue,
            };

            if game.phase != GamePhase::Voting {
                continue;
            }

            // Check if deadline has passed
            let deadline = match &game.phase_deadline {
                Some(d) => d,
                None => continue,
            };

            let deadline_time = match chrono::DateTime::parse_from_rfc3339(deadline) {
                Ok(dt) => dt,
                Err(_) => continue,
            };

            let now = chrono::Utc::now();
            if now < deadline_time {
                continue;
            }

            // Deadline has passed - auto-advance to RESULTS
            tracing::info!("Voting deadline expired, auto-advancing to RESULTS");

            match state.transition_phase(GamePhase::Results).await {
                Ok(_) => {
                    tracing::info!("Auto-transitioned to RESULTS phase");
                }
                Err(e) => {
                    // Log error but don't crash - maybe AI submission isn't set
                    tracing::warn!("Failed to auto-transition to RESULTS: {}", e);
                }
            }
        }
    });
}
