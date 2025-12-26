//! Player message handlers
//!
//! Handlers for player-specific messages like registration, submission, and typo checking.

use crate::protocol::{PlayerSubmissionStatus, ServerMessage};
use crate::state::AppState;
use crate::ws::host::broadcast_player_status_to_host;
use std::sync::Arc;

pub async fn handle_register_player(
    state: &Arc<AppState>,
    player_token: String,
    display_name: String,
) -> Option<ServerMessage> {
    tracing::info!("Player registration: {}", display_name);
    match state
        .register_player(&player_token, display_name.clone())
        .await
    {
        Ok(player) => {
            broadcast_player_status_to_host(state).await;
            Some(ServerMessage::PlayerRegistered {
                player_id: player.id,
                display_name,
            })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "REGISTRATION_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_submit_answer(
    state: &Arc<AppState>,
    player_token: Option<String>,
    text: String,
) -> Option<ServerMessage> {
    tracing::info!("Answer submitted: {}", text);
    let round = state.get_current_round().await?;

    let player_id = if let Some(token) = player_token {
        match state.get_player_by_token(&token).await {
            Some(player) => Some(player.id),
            None => {
                return Some(ServerMessage::Error {
                    code: "INVALID_PLAYER_TOKEN".to_string(),
                    msg: "Ungültiger Token".to_string(),
                });
            }
        }
    } else {
        None
    };

    match state.submit_answer(&round.id, player_id, text).await {
        Ok(_) => {
            broadcast_player_status_to_host(state).await;
            Some(ServerMessage::SubmissionConfirmed)
        }
        Err(e) => Some(ServerMessage::Error {
            code: "SUBMISSION_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_request_typo_check(
    state: &Arc<AppState>,
    player_token: String,
    text: String,
) -> Option<ServerMessage> {
    tracing::info!(
        "Typo check requested for text: {}",
        &text[..text.len().min(50)]
    );

    // Validate player token
    let player = match state.get_player_by_token(&player_token).await {
        Some(p) => p,
        None => {
            return Some(ServerMessage::Error {
                code: "INVALID_PLAYER_TOKEN".to_string(),
                msg: "Ungültiger Token".to_string(),
            });
        }
    };

    // Set player status to checking typos
    state
        .set_player_status(&player.id, PlayerSubmissionStatus::CheckingTypos)
        .await;
    broadcast_player_status_to_host(state).await;

    // Check if we have an LLM provider
    let llm = match &state.llm {
        Some(llm) => llm,
        None => {
            tracing::warn!("No LLM provider available for typo check, returning original");
            state
                .set_player_status(&player.id, PlayerSubmissionStatus::NotSubmitted)
                .await;
            broadcast_player_status_to_host(state).await;
            return Some(ServerMessage::TypoCheckResult {
                original: text.clone(),
                corrected: text,
                has_changes: false,
            });
        }
    };

    let providers = &llm.providers;
    if providers.is_empty() {
        tracing::warn!("No LLM providers configured for typo check");
        state
            .set_player_status(&player.id, PlayerSubmissionStatus::NotSubmitted)
            .await;
        broadcast_player_status_to_host(state).await;
        return Some(ServerMessage::TypoCheckResult {
            original: text.clone(),
            corrected: text,
            has_changes: false,
        });
    }

    // Run typo check
    let corrected = crate::llm::check_typos(providers[0].as_ref(), &text).await;

    // Clear the checking status
    state
        .set_player_status(&player.id, PlayerSubmissionStatus::NotSubmitted)
        .await;
    broadcast_player_status_to_host(state).await;

    let has_changes = corrected != text;
    tracing::info!(
        "Typo check complete. Has changes: {}, original len: {}, corrected len: {}",
        has_changes,
        text.len(),
        corrected.len()
    );

    Some(ServerMessage::TypoCheckResult {
        original: text,
        corrected,
        has_changes,
    })
}

pub async fn handle_update_submission(
    state: &Arc<AppState>,
    player_token: String,
    submission_id: String,
    new_text: String,
) -> Option<ServerMessage> {
    tracing::info!("Submission update requested: {}", submission_id);

    // Validate player token
    let player = match state.get_player_by_token(&player_token).await {
        Some(p) => p,
        None => {
            return Some(ServerMessage::Error {
                code: "INVALID_PLAYER_TOKEN".to_string(),
                msg: "Ungültiger Token".to_string(),
            });
        }
    };

    match state
        .update_player_submission(&submission_id, &player.id, new_text)
        .await
    {
        Ok(_) => Some(ServerMessage::SubmissionConfirmed),
        Err(e) => Some(ServerMessage::Error {
            code: "UPDATE_SUBMISSION_FAILED".to_string(),
            msg: e,
        }),
    }
}
