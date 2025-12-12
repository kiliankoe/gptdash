//! Host-only command handlers
//!
//! All handlers in this module require the Host role.
//! Authorization is checked in the main dispatch layer before calling these.

use crate::protocol::{AiGenStatus, ManualWinnerType, ServerMessage, SubmissionInfo};
use crate::state::AppState;
use std::sync::Arc;

/// Broadcast current player status to host
pub async fn broadcast_player_status_to_host(state: &Arc<AppState>) {
    let players = state.get_all_player_status().await;
    state.broadcast_to_host(ServerMessage::HostPlayerStatus { players });
}

pub async fn handle_create_players(state: &Arc<AppState>, count: u32) -> Option<ServerMessage> {
    tracing::info!("Host creating {} players", count);
    let mut players = Vec::new();
    for _ in 0..count {
        let player = state.create_player().await;
        players.push(crate::protocol::PlayerToken {
            id: player.id,
            token: player.token,
        });
    }
    // Broadcast updated player status to host
    broadcast_player_status_to_host(state).await;
    Some(ServerMessage::PlayersCreated { players })
}

pub async fn handle_transition_phase(
    state: &Arc<AppState>,
    phase: crate::types::GamePhase,
) -> Option<ServerMessage> {
    tracing::info!("Host transitioning to phase: {:?}", phase);
    match state.transition_phase(phase.clone()).await {
        Ok(_) => {
            let game = state.get_game().await?;
            let valid_transitions = AppState::get_valid_transitions(&game.phase);
            // Include prompt when transitioning to WRITING so host has it
            let prompt = if game.phase == crate::types::GamePhase::Writing {
                state
                    .get_current_round()
                    .await
                    .and_then(|r| r.selected_prompt)
            } else {
                None
            };
            Some(ServerMessage::Phase {
                phase: game.phase,
                round_no: game.round_no,
                server_now: chrono::Utc::now().to_rfc3339(),
                deadline: game.phase_deadline,
                valid_transitions,
                prompt,
            })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "TRANSITION_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_start_round(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host starting new round");
    match state.start_round().await {
        Ok(round) => Some(ServerMessage::RoundStarted { round }),
        Err(e) => Some(ServerMessage::Error {
            code: "ROUND_START_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_select_prompt(
    state: &Arc<AppState>,
    prompt_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host selecting prompt: {}", prompt_id);

    // Ensure we have a round to select the prompt for
    let (round, is_new_round) = match state.get_current_round().await {
        Some(r) => {
            // Check if round is in Setup state, if not start a new one
            if r.state != crate::types::RoundState::Setup {
                tracing::info!("Current round not in Setup state, starting new round");
                match state.start_round().await {
                    Ok(new_round) => (new_round, true),
                    Err(e) => {
                        return Some(ServerMessage::Error {
                            code: "PROMPT_SELECT_FAILED".to_string(),
                            msg: format!("Failed to start new round: {}", e),
                        });
                    }
                }
            } else {
                (r, false)
            }
        }
        None => {
            // No round exists, create one
            tracing::info!("No current round, creating one for prompt selection");
            match state.start_round().await {
                Ok(new_round) => (new_round, true),
                Err(e) => {
                    return Some(ServerMessage::Error {
                        code: "PROMPT_SELECT_FAILED".to_string(),
                        msg: format!("Failed to start round: {}", e),
                    });
                }
            }
        }
    };

    // If we created a new round, broadcast it to all clients
    if is_new_round {
        state.broadcast_to_all(ServerMessage::RoundStarted {
            round: round.clone(),
        });
    }

    // Select prompt from pool (removes it from pool)
    match state.select_prompt(&round.id, &prompt_id).await {
        Ok(prompt) => {
            // Broadcast updated pool to host
            state.broadcast_prompts_to_host().await;
            // Broadcast to all clients so beamer and players can update their displays
            state.broadcast_to_all(ServerMessage::PromptSelected {
                prompt: prompt.clone(),
            });
            Some(ServerMessage::PromptSelected { prompt })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_SELECT_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_edit_submission(
    state: &Arc<AppState>,
    submission_id: String,
    new_text: String,
) -> Option<ServerMessage> {
    tracing::info!("Host editing submission: {}", submission_id);
    match state.edit_submission(&submission_id, new_text).await {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            code: "EDIT_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_set_reveal_order(
    state: &Arc<AppState>,
    order: Vec<String>,
) -> Option<ServerMessage> {
    tracing::info!("Host setting reveal order: {} items", order.len());
    let round = state.get_current_round().await?;

    match state.set_reveal_order(&round.id, order).await {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            code: "REVEAL_ORDER_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_set_ai_submission(
    state: &Arc<AppState>,
    submission_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host setting AI submission: {}", submission_id);
    let round = state.get_current_round().await?;

    match state.set_ai_submission(&round.id, submission_id).await {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            code: "SET_AI_SUBMISSION_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_reveal_next(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host advancing reveal");
    let round = state.get_current_round().await?;

    match state.reveal_next(&round.id).await {
        Ok(reveal_index) => {
            let submission = state.get_current_reveal_submission(&round.id).await;
            let submission_info = submission.map(|s| SubmissionInfo::from(&s));

            // Broadcast to all clients
            state.broadcast_to_all(ServerMessage::RevealUpdate {
                reveal_index,
                submission: submission_info.clone(),
            });

            Some(ServerMessage::RevealUpdate {
                reveal_index,
                submission: submission_info,
            })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "REVEAL_NEXT_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_reveal_prev(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host going back in reveal");
    let round = state.get_current_round().await?;

    match state.reveal_prev(&round.id).await {
        Ok(reveal_index) => {
            let submission = state.get_current_reveal_submission(&round.id).await;
            let submission_info = submission.map(|s| SubmissionInfo::from(&s));

            // Broadcast to all clients
            state.broadcast_to_all(ServerMessage::RevealUpdate {
                reveal_index,
                submission: submission_info.clone(),
            });

            Some(ServerMessage::RevealUpdate {
                reveal_index,
                submission: submission_info,
            })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "REVEAL_PREV_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_reset_game(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host resetting game");
    state.reset_game().await;

    let game = state.get_game().await?;
    let valid_transitions = AppState::get_valid_transitions(&game.phase);
    Some(ServerMessage::GameState {
        game,
        valid_transitions,
    })
}

pub async fn handle_clear_prompt_pool(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host clearing prompt pool");
    state.clear_prompt_pool().await;

    // Broadcast empty prompts list to host
    state.broadcast_prompts_to_host().await;

    None
}

pub async fn handle_add_prompt(
    state: &Arc<AppState>,
    text: Option<String>,
    image_url: Option<String>,
) -> Option<ServerMessage> {
    let is_multimodal = image_url.is_some();
    tracing::info!(
        "Host adding prompt to pool: {:?} (multimodal: {})",
        text.as_deref().unwrap_or("(image only)"),
        is_multimodal
    );

    // Add prompt to the global pool (no round needed)
    match state
        .add_prompt_to_pool(text, image_url, crate::types::PromptSource::Host, None)
        .await
    {
        Ok(prompt) => {
            // Broadcast updated prompts list to host so UI updates
            state.broadcast_prompts_to_host().await;
            tracing::info!("Prompt added to pool: {}", prompt.id);
            None // Just acknowledge, don't auto-select
        }
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_ADD_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_toggle_panic_mode(
    state: &Arc<AppState>,
    enabled: bool,
) -> Option<ServerMessage> {
    tracing::info!("Host toggling panic mode: {}", enabled);
    state.set_panic_mode(enabled).await;
    Some(ServerMessage::PanicModeUpdate { enabled })
}

pub async fn handle_set_manual_winner(
    state: &Arc<AppState>,
    winner_type: ManualWinnerType,
    submission_id: String,
) -> Option<ServerMessage> {
    tracing::info!(
        "Host setting manual {:?} winner: {}",
        winner_type,
        submission_id
    );

    let round = match state.get_current_round().await {
        Some(r) => r,
        None => {
            return Some(ServerMessage::Error {
                code: "NO_ACTIVE_ROUND".to_string(),
                msg: "No active round".to_string(),
            });
        }
    };

    let result = match winner_type {
        ManualWinnerType::Ai => state.set_manual_ai_winner(&round.id, submission_id).await,
        ManualWinnerType::Funny => {
            state
                .set_manual_funny_winner(&round.id, submission_id)
                .await
        }
    };

    match result {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            code: "SET_MANUAL_WINNER_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_mark_duplicate(
    state: &Arc<AppState>,
    submission_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host marking submission as duplicate: {}", submission_id);

    match state.mark_submission_duplicate(&submission_id).await {
        Ok(player_id) => {
            if let Some(pid) = player_id {
                state.broadcast_to_all(ServerMessage::SubmissionRejected {
                    player_id: pid,
                    reason: "duplicate".to_string(),
                });
            }
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "MARK_DUPLICATE_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_extend_timer(state: &Arc<AppState>, seconds: u32) -> Option<ServerMessage> {
    tracing::info!("Host extending timer by {} seconds", seconds);

    match state.extend_deadline(seconds).await {
        Ok(new_deadline) => {
            let server_now = chrono::Utc::now().to_rfc3339();
            state.broadcast_to_all(ServerMessage::DeadlineUpdate {
                deadline: new_deadline.clone(),
                server_now: server_now.clone(),
            });
            Some(ServerMessage::DeadlineUpdate {
                deadline: new_deadline,
                server_now,
            })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "EXTEND_TIMER_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_regenerate_ai(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host requesting AI regeneration");

    let round = match state.get_current_round().await {
        Some(r) => r,
        None => {
            return Some(ServerMessage::Error {
                code: "NO_ACTIVE_ROUND".to_string(),
                msg: "No active round".to_string(),
            });
        }
    };

    let prompt = match &round.selected_prompt {
        Some(p) => {
            // Validate prompt has either text or image
            if p.text.is_none() && p.image_url.is_none() {
                return Some(ServerMessage::Error {
                    code: "INVALID_PROMPT".to_string(),
                    msg: "Selected prompt has neither text nor image".to_string(),
                });
            }
            p.clone()
        }
        None => {
            return Some(ServerMessage::Error {
                code: "NO_PROMPT_SELECTED".to_string(),
                msg: "No prompt selected for this round".to_string(),
            });
        }
    };

    // Notify host that generation is starting
    let is_multimodal = prompt.image_url.is_some();
    state.broadcast_to_host(ServerMessage::AiGenerationStatus {
        status: AiGenStatus::Started,
        provider: None,
        message: Some(if is_multimodal {
            "Generating AI submissions (multimodal)...".to_string()
        } else {
            "Generating AI submissions...".to_string()
        }),
    });

    // Spawn generation in background
    let state_clone = state.clone();
    let round_id = round.id.clone();
    tokio::spawn(async move {
        match state_clone
            .generate_ai_submissions(&round_id, &prompt)
            .await
        {
            Ok(_) => {
                state_clone.broadcast_to_host(ServerMessage::AiGenerationStatus {
                    status: AiGenStatus::Completed,
                    provider: None,
                    message: Some("AI generation completed".to_string()),
                });
            }
            Err(e) => {
                state_clone.broadcast_to_host(ServerMessage::AiGenerationStatus {
                    status: AiGenStatus::AllFailed,
                    provider: None,
                    message: Some(e),
                });
            }
        }
    });

    None
}

pub async fn handle_write_ai_submission(
    state: &Arc<AppState>,
    text: String,
) -> Option<ServerMessage> {
    tracing::info!("Host writing manual AI submission");

    let round = match state.get_current_round().await {
        Some(r) => r,
        None => {
            return Some(ServerMessage::Error {
                code: "NO_ACTIVE_ROUND".to_string(),
                msg: "No active round".to_string(),
            });
        }
    };

    match state
        .create_manual_ai_submission(&round.id, text.clone())
        .await
    {
        Ok(submission) => {
            state.broadcast_submissions(&round.id).await;
            tracing::info!("Manual AI submission created: {}", submission.id);
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "WRITE_AI_SUBMISSION_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_shadowban_audience(
    state: &Arc<AppState>,
    voter_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host shadowbanning audience member: {}", voter_id);
    state.shadowban_audience(voter_id.clone()).await;

    // Re-broadcast prompts to host (now filtered)
    state.broadcast_prompts_to_host().await;

    None
}

pub async fn handle_shadowban_prompt_submitters(
    state: &Arc<AppState>,
    prompt_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host shadowbanning all submitters of prompt: {}", prompt_id);

    match state.shadowban_prompt_submitters(&prompt_id).await {
        Ok(banned_ids) => {
            tracing::info!(
                "Shadowbanned {} users for prompt {}",
                banned_ids.len(),
                prompt_id
            );
            // Re-broadcast prompts to host (now filtered)
            state.broadcast_prompts_to_host().await;
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "SHADOWBAN_PROMPT_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_remove_player(
    state: &Arc<AppState>,
    player_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host removing player: {}", player_id);

    match state.remove_player(&player_id).await {
        Ok(_player) => {
            state.broadcast_to_all(ServerMessage::PlayerRemoved {
                player_id: player_id.clone(),
            });
            broadcast_player_status_to_host(state).await;
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "REMOVE_PLAYER_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_queue_prompt(
    state: &Arc<AppState>,
    prompt_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host queuing prompt: {}", prompt_id);

    match state.queue_prompt(&prompt_id).await {
        Ok(_prompt) => {
            // Broadcast updated pool and queue to host
            state.broadcast_prompts_to_host().await;
            state.broadcast_queued_prompts_to_host().await;
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "QUEUE_PROMPT_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_unqueue_prompt(
    state: &Arc<AppState>,
    prompt_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host unqueuing prompt: {}", prompt_id);

    match state.unqueue_prompt(&prompt_id).await {
        Ok(_prompt) => {
            // Broadcast updated pool and queue to host
            state.broadcast_prompts_to_host().await;
            state.broadcast_queued_prompts_to_host().await;
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "UNQUEUE_PROMPT_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_delete_prompt(
    state: &Arc<AppState>,
    prompt_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host deleting prompt: {}", prompt_id);

    if state.delete_prompt(&prompt_id).await {
        // Broadcast updated pool and queue to host
        state.broadcast_prompts_to_host().await;
        state.broadcast_queued_prompts_to_host().await;
        None
    } else {
        Some(ServerMessage::Error {
            code: "DELETE_PROMPT_FAILED".to_string(),
            msg: "Prompt not found".to_string(),
        })
    }
}
