//! Audience message handlers
//!
//! Handlers for audience-specific messages like voting and prompt submission.

use crate::protocol::ServerMessage;
use crate::state::vote::VoteResult;
use crate::state::AppState;
use std::sync::Arc;

#[allow(clippy::too_many_arguments)]
pub async fn handle_vote(
    state: &Arc<AppState>,
    voter_token: String,
    ai: String,
    funny: String,
    msg_id: String,
    challenge_nonce: String,
    challenge_response: String,
    is_webdriver: bool,
) -> Option<ServerMessage> {
    tracing::info!("Vote: AI={}, Funny={}, MsgID={}", ai, funny, msg_id);

    // Verify challenge first (anti-automation)
    let expected_nonce = state.get_vote_challenge_nonce().await;
    match expected_nonce {
        Some(ref nonce) => {
            if !AppState::verify_vote_challenge(
                nonce,
                &voter_token,
                &challenge_nonce,
                &challenge_response,
            ) {
                tracing::warn!(
                    "Vote challenge failed for voter {}: nonce={}, response={}",
                    voter_token,
                    challenge_nonce,
                    challenge_response
                );
                return Some(ServerMessage::Error {
                    code: "CHALLENGE_FAILED".to_string(),
                    msg: "Ungültige Abstimmung. Bitte Seite neu laden.".to_string(),
                });
            }
        }
        None => {
            // No challenge set (shouldn't happen during VOTING phase, but be defensive)
            tracing::warn!(
                "No vote challenge nonce set, rejecting vote from {}",
                voter_token
            );
            return Some(ServerMessage::Error {
                code: "CHALLENGE_FAILED".to_string(),
                msg: "Ungültige Abstimmung. Bitte Seite neu laden.".to_string(),
            });
        }
    }

    // Shadow rejection: silently discard suspicious votes (return VoteAck to not alert attacker)
    // SKIP_VOTE_ANTI_AUTOMATION=1 disables all anti-automation checks (for e2e/integration tests)
    let skip_anti_automation = std::env::var("SKIP_VOTE_ANTI_AUTOMATION")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);

    if !skip_anti_automation {
        // 1. Webdriver detection (catches unpatched Playwright/Puppeteer)
        if is_webdriver {
            tracing::warn!(
                voter = %voter_token,
                "Vote silently discarded: webdriver detected"
            );
            return Some(ServerMessage::VoteAck {
                msg_id: msg_id.clone(),
            });
        }

        // 2. Server-side timing check (discard impossibly fast votes)
        if let Some(voting_started) = state.get_voting_phase_started_at().await {
            let server_delta_ms = chrono::Utc::now()
                .signed_duration_since(voting_started)
                .num_milliseconds();

            if (0..500).contains(&server_delta_ms) {
                tracing::warn!(
                    voter = %voter_token,
                    delta_ms = server_delta_ms,
                    "Vote silently discarded: too fast"
                );
                return Some(ServerMessage::VoteAck {
                    msg_id: msg_id.clone(),
                });
            }
        }
    }

    // If we get here, vote is legitimate - proceed with actual storage
    match state
        .submit_vote(voter_token, ai, funny, msg_id.clone())
        .await
    {
        VoteResult::Recorded => {
            tracing::debug!("Vote recorded");
            Some(ServerMessage::VoteAck { msg_id })
        }
        VoteResult::Duplicate => {
            tracing::debug!("Duplicate vote msg_id, returning ack");
            Some(ServerMessage::VoteAck { msg_id })
        }
        VoteResult::NoActiveRound => {
            tracing::warn!("Vote received but no active round");
            Some(ServerMessage::VoteAck { msg_id })
        }
        VoteResult::PanicModeActive => {
            tracing::info!("Vote rejected: panic mode active");
            Some(ServerMessage::Error {
                code: "PANIC_MODE".to_string(),
                msg: "Voting is temporarily disabled".to_string(),
            })
        }
        VoteResult::WrongPhase => {
            tracing::info!("Vote rejected: not in voting phase");
            Some(ServerMessage::Error {
                code: "WRONG_PHASE".to_string(),
                msg: "Voting is only allowed during the voting phase".to_string(),
            })
        }
        VoteResult::InvalidPick => {
            tracing::info!("Vote rejected: invalid pick(s)");
            Some(ServerMessage::Error {
                code: "INVALID_VOTE".to_string(),
                msg: "Invalid vote. Please pick two different answers from this round.".to_string(),
            })
        }
        VoteResult::UnknownVoter => {
            // Shadow reject - don't tell attacker their fabricated token was detected
            tracing::warn!("Vote from unknown voter (fabricated token?)");
            Some(ServerMessage::VoteAck { msg_id })
        }
        VoteResult::AlreadyVoted => {
            tracing::info!("Vote rejected: voter already voted this round");
            Some(ServerMessage::Error {
                code: "ALREADY_VOTED".to_string(),
                msg: "Du hast in dieser Runde bereits abgestimmt.".to_string(),
            })
        }
    }
}

pub async fn handle_submit_prompt(
    state: &Arc<AppState>,
    voter_token: Option<String>,
    text: String,
) -> Option<ServerMessage> {
    tracing::info!("Prompt submitted: {}", text);

    // Check if this voter is shadowbanned first
    if let Some(ref token) = voter_token {
        if state.is_shadowbanned(token).await {
            tracing::info!(
                "Shadowbanned voter {} submitted prompt, silently ignoring",
                token
            );
            // Return success to the user so they don't know they're shadowbanned
            return None;
        }
    }

    // Add prompt directly to the global pool (no round needed)
    match state
        .add_prompt_to_pool(
            Some(text),
            None, // Audience prompts don't support images
            crate::types::PromptSource::Audience,
            voter_token.clone(),
        )
        .await
    {
        Ok(prompt) => {
            state.broadcast_prompts_to_host().await;
            tracing::info!("Prompt added to pool: {}", prompt.id);
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_prompt_vote(
    state: &Arc<AppState>,
    voter_token: String,
    prompt_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Prompt vote: {} for {}", voter_token, prompt_id);

    // Check we're in PROMPT_SELECTION phase
    let game = state.get_game().await;
    if let Some(game) = game {
        if game.phase != crate::types::GamePhase::PromptSelection {
            return Some(ServerMessage::Error {
                code: "WRONG_PHASE".to_string(),
                msg: "Prompt voting only available during prompt selection".to_string(),
            });
        }
    } else {
        return Some(ServerMessage::Error {
            code: "NO_GAME".to_string(),
            msg: "No active game".to_string(),
        });
    }

    // Check if shadowbanned
    if state.is_shadowbanned(&voter_token).await {
        tracing::info!(
            "Shadowbanned voter {} tried to vote on prompt, silently ignoring",
            voter_token
        );
        return Some(ServerMessage::PromptVoteAck);
    }

    // Record the vote
    match state.record_prompt_vote(&voter_token, &prompt_id).await {
        Ok(_) => Some(ServerMessage::PromptVoteAck),
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_VOTE_FAILED".to_string(),
            msg: e,
        }),
    }
}

// ========== Trivia System ==========

pub async fn handle_submit_trivia_vote(
    state: &Arc<AppState>,
    voter_token: String,
    choice_index: usize,
) -> Option<ServerMessage> {
    tracing::info!("Trivia vote from {}: choice {}", voter_token, choice_index);

    // Check we're in WRITING phase (trivia only active during WRITING)
    let game = state.get_game().await;
    if let Some(game) = game {
        if game.phase != crate::types::GamePhase::Writing {
            return Some(ServerMessage::Error {
                code: "WRONG_PHASE".to_string(),
                msg: "Trivia voting only available during writing phase".to_string(),
            });
        }
    } else {
        return Some(ServerMessage::Error {
            code: "NO_GAME".to_string(),
            msg: "No active game".to_string(),
        });
    }

    // Get active trivia question ID for the response
    let question_id = match state.get_active_trivia_id().await {
        Some(id) => id,
        None => {
            return Some(ServerMessage::Error {
                code: "NO_ACTIVE_TRIVIA".to_string(),
                msg: "No trivia question is currently active".to_string(),
            });
        }
    };

    // Submit the vote
    match state.submit_trivia_vote(&voter_token, choice_index).await {
        Ok(_) => {
            // Notify host of updated vote count
            crate::ws::host::broadcast_trivia_to_host(state).await;

            Some(ServerMessage::TriviaVoteAck { question_id })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "TRIVIA_VOTE_FAILED".to_string(),
            msg: e,
        }),
    }
}
