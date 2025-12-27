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
    model: Option<String>,
) -> Option<ServerMessage> {
    tracing::info!(
        "Host transitioning to phase: {:?} with model: {:?}",
        phase,
        model
    );
    match state.transition_phase(phase.clone(), model).await {
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
    model: Option<String>,
) -> Option<ServerMessage> {
    tracing::info!(
        "Host selecting prompt: {} with model: {:?}",
        prompt_id,
        model
    );

    // Ensure we have a round to select the prompt for
    let round = match state.get_current_round().await {
        Some(r) => {
            // Check if round is in Setup state, if not start a new one
            if r.state != crate::types::RoundState::Setup {
                tracing::info!("Current round not in Setup state, starting new round");
                match state.start_round().await {
                    Ok(new_round) => new_round,
                    Err(e) => {
                        return Some(ServerMessage::Error {
                            code: "PROMPT_SELECT_FAILED".to_string(),
                            msg: format!("Failed to start new round: {}", e),
                        });
                    }
                }
            } else {
                r
            }
        }
        None => {
            // No round exists, create one
            tracing::info!("No current round, creating one for prompt selection");
            match state.start_round().await {
                Ok(new_round) => new_round,
                Err(e) => {
                    return Some(ServerMessage::Error {
                        code: "PROMPT_SELECT_FAILED".to_string(),
                        msg: format!("Failed to start round: {}", e),
                    });
                }
            }
        }
    };

    // Select prompt from pool (removes it from pool)
    match state.select_prompt(&round.id, &prompt_id, model).await {
        Ok(prompt) => Some(ServerMessage::PromptSelected { prompt }),
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

pub async fn handle_clear_audience_members(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host clearing audience members");
    state.clear_audience_members().await;
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

pub async fn handle_toggle_soft_panic_mode(
    state: &Arc<AppState>,
    enabled: bool,
) -> Option<ServerMessage> {
    tracing::info!("Host toggling soft panic mode: {}", enabled);
    state.set_soft_panic_mode(enabled).await;
    Some(ServerMessage::SoftPanicModeUpdate { enabled })
}

pub async fn handle_toggle_venue_only_mode(
    state: &Arc<AppState>,
    enabled: bool,
) -> Option<ServerMessage> {
    tracing::info!("Host toggling venue-only mode: {}", enabled);
    state.set_venue_only_mode(enabled).await;
    Some(ServerMessage::VenueOnlyModeUpdate { enabled })
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

pub async fn handle_regenerate_ai(
    state: &Arc<AppState>,
    model: Option<String>,
) -> Option<ServerMessage> {
    tracing::info!("Host requesting AI regeneration with model: {:?}", model);

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

    // Regenerating AI after revealing/voting can invalidate reveal order and votes.
    // Keep this operation constrained to the collecting phase.
    if round.state != crate::types::RoundState::Collecting {
        return Some(ServerMessage::Error {
            code: "INVALID_ROUND_STATE".to_string(),
            msg: format!(
                "Can only regenerate AI submissions while collecting (currently: {:?})",
                round.state
            ),
        });
    }

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
    let model_clone = model.clone();
    tokio::spawn(async move {
        match state_clone
            .generate_ai_submissions(&round_id, &prompt, model_clone.as_deref())
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

pub async fn handle_remove_submission(
    state: &Arc<AppState>,
    submission_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host removing submission: {}", submission_id);

    match state.remove_submission(&submission_id).await {
        Ok(player_id) => {
            if let Some(pid) = player_id {
                state.broadcast_to_all(ServerMessage::SubmissionRejected {
                    player_id: pid,
                    reason: "removed".to_string(),
                });
            }
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "REMOVE_SUBMISSION_FAILED".to_string(),
            msg: e,
        }),
    }
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

// ========== Trivia System ==========

/// Broadcast current trivia questions to host
pub async fn broadcast_trivia_to_host(state: &Arc<AppState>) {
    let questions = state.get_trivia_questions().await;
    let active_trivia_id = state.get_active_trivia_id().await;
    let active_trivia_votes = state.get_active_trivia_vote_count().await;

    tracing::info!(
        "Broadcasting trivia to host: active_id={:?}, votes={}",
        active_trivia_id,
        active_trivia_votes
    );

    state.broadcast_to_host(ServerMessage::HostTriviaQuestions {
        questions,
        active_trivia_id,
        active_trivia_votes,
    });
}

pub async fn handle_add_trivia_question(
    state: &Arc<AppState>,
    question: String,
    image_url: Option<String>,
    choices: Vec<crate::protocol::TriviaChoiceInput>,
) -> Option<ServerMessage> {
    tracing::info!("Host adding trivia question: {:?}", question);

    // Validate 2-4 choices
    if choices.len() < 2 || choices.len() > 4 {
        return Some(ServerMessage::Error {
            code: "INVALID_TRIVIA_QUESTION".to_string(),
            msg: format!(
                "Trivia questions must have 2-4 choices, got {}",
                choices.len()
            ),
        });
    }

    // Convert protocol choices to state choices
    let state_choices: Vec<crate::state::trivia::TriviaChoiceInput> = choices
        .into_iter()
        .map(|c| crate::state::trivia::TriviaChoiceInput {
            text: c.text,
            image_url: c.image_url,
            is_correct: c.is_correct,
        })
        .collect();

    match state
        .add_trivia_question(question, image_url, state_choices)
        .await
    {
        Ok(_trivia) => {
            // Broadcast updated trivia list to host
            broadcast_trivia_to_host(state).await;
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "ADD_TRIVIA_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_remove_trivia_question(
    state: &Arc<AppState>,
    question_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host removing trivia question: {}", question_id);

    if state.remove_trivia_question(&question_id).await {
        // Broadcast updated trivia list to host
        broadcast_trivia_to_host(state).await;
        None
    } else {
        Some(ServerMessage::Error {
            code: "REMOVE_TRIVIA_FAILED".to_string(),
            msg: "Trivia question not found".to_string(),
        })
    }
}

pub async fn handle_present_trivia(
    state: &Arc<AppState>,
    question_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host presenting trivia question: {}", question_id);

    match state.present_trivia(&question_id).await {
        Ok(question) => {
            // Broadcast trivia question to all clients (beamer + audience)
            let choices: Vec<crate::protocol::TriviaChoiceOutput> = question
                .choices
                .iter()
                .map(|c| crate::protocol::TriviaChoiceOutput {
                    text: c.text.clone(),
                    image_url: c.image_url.clone(),
                })
                .collect();
            let msg = ServerMessage::TriviaQuestion {
                question_id: question.id.clone(),
                question: question.question.clone(),
                image_url: question.image_url.clone(),
                choices,
            };
            state.broadcast_to_all(msg);

            // Update host with active trivia
            broadcast_trivia_to_host(state).await;

            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "PRESENT_TRIVIA_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_resolve_trivia(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host resolving trivia question");

    match state.resolve_trivia().await {
        Some(result) => {
            // Convert choice data to protocol output format
            let choices: Vec<crate::protocol::TriviaChoiceOutput> = result
                .choices
                .into_iter()
                .map(|c| crate::protocol::TriviaChoiceOutput {
                    text: c.text,
                    image_url: c.image_url,
                })
                .collect();

            // Broadcast results to all clients (beamer + audience)
            let msg = ServerMessage::TriviaResult {
                question_id: result.question_id,
                question: result.question,
                image_url: result.image_url,
                choices,
                correct_indices: result.correct_indices,
                vote_counts: result.vote_counts,
                total_votes: result.total_votes,
            };
            state.broadcast_to_all(msg);

            // Update host (active trivia is now cleared)
            broadcast_trivia_to_host(state).await;

            None
        }
        None => Some(ServerMessage::Error {
            code: "RESOLVE_TRIVIA_FAILED".to_string(),
            msg: "No active trivia question to resolve".to_string(),
        }),
    }
}

pub async fn handle_clear_trivia(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host clearing trivia without resolving");

    state.clear_trivia().await;

    // Broadcast clear to all clients (beamer + audience)
    state.broadcast_to_all(ServerMessage::TriviaClear);

    // Update host
    broadcast_trivia_to_host(state).await;

    None
}

pub async fn handle_reveal_vote_labels(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host revealing vote labels on beamer");

    // Broadcast to beamer to reveal the vote labels
    state.broadcast_to_beamer(ServerMessage::VoteLabelsRevealed);

    None
}

// ========== Score Editing ==========

pub async fn handle_edit_player_score(
    state: &Arc<AppState>,
    player_id: String,
    ai_detect_points: u32,
    funny_points: u32,
) -> Option<ServerMessage> {
    tracing::info!(
        "Host editing player score: {} -> AI: {}, Funny: {}",
        player_id,
        ai_detect_points,
        funny_points
    );

    match state
        .edit_player_score(&player_id, ai_detect_points, funny_points)
        .await
    {
        Ok(_) => {
            // Broadcast updated leaderboards to host and beamer
            let (players, audience) = state.get_leaderboards().await;
            let msg = ServerMessage::Scores {
                players,
                audience_top: audience.into_iter().take(10).collect(),
                ai_submission_id: None,
            };
            state.broadcast_to_host(msg.clone());
            state.broadcast_to_beamer(msg);
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "EDIT_PLAYER_SCORE_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_clear_audience_score(
    state: &Arc<AppState>,
    voter_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host clearing audience score: {}", voter_id);

    match state.clear_audience_score(&voter_id).await {
        Ok(_) => {
            // Broadcast updated leaderboards to host and beamer
            let (players, audience) = state.get_leaderboards().await;
            let msg = ServerMessage::Scores {
                players,
                audience_top: audience.into_iter().take(10).collect(),
                ai_submission_id: None,
            };
            state.broadcast_to_host(msg.clone());
            state.broadcast_to_beamer(msg);
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "CLEAR_AUDIENCE_SCORE_FAILED".to_string(),
            msg: e,
        }),
    }
}

pub async fn handle_edit_audience_score(
    state: &Arc<AppState>,
    voter_id: String,
    ai_detect_points: u32,
) -> Option<ServerMessage> {
    tracing::info!(
        "Host editing audience score: {} -> AI: {}",
        voter_id,
        ai_detect_points
    );

    match state.edit_audience_score(&voter_id, ai_detect_points).await {
        Ok(_) => {
            // Broadcast updated leaderboards to host and beamer
            let (players, audience) = state.get_leaderboards().await;
            let msg = ServerMessage::Scores {
                players,
                audience_top: audience.into_iter().take(10).collect(),
                ai_submission_id: None,
            };
            state.broadcast_to_host(msg.clone());
            state.broadcast_to_beamer(msg);
            None
        }
        Err(e) => Some(ServerMessage::Error {
            code: "EDIT_AUDIENCE_SCORE_FAILED".to_string(),
            msg: e,
        }),
    }
}
