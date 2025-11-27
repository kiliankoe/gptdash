use crate::protocol::{ClientMessage, ServerMessage};
use crate::state::AppState;
use crate::types::Role;
use std::sync::Arc;

/// Handle client messages and return optional response
pub async fn handle_message(
    msg: ClientMessage,
    role: &Role,
    state: &Arc<AppState>,
) -> Option<ServerMessage> {
    match msg {
        ClientMessage::Join { room_token } => {
            tracing::info!("Join request with token: {}", room_token);
            None
        }

        ClientMessage::RegisterPlayer {
            player_token,
            display_name,
        } => handle_register_player(state, player_token, display_name).await,

        ClientMessage::SubmitAnswer { player_token, text } => {
            handle_submit_answer(state, player_token, text).await
        }

        ClientMessage::Vote {
            voter_token,
            ai,
            funny,
            msg_id,
        } => handle_vote(state, voter_token, ai, funny, msg_id).await,

        ClientMessage::SubmitPrompt { text } => handle_submit_prompt(state, text).await,

        ClientMessage::AckNeeded {
            last_seen_server_seq,
        } => {
            tracing::info!("Ack needed: seq={}", last_seen_server_seq);
            None
        }

        // Host-only commands
        ClientMessage::HostCreatePlayers { count } => {
            if *role != Role::Host {
                return unauthorized("Only host can create players");
            }
            handle_host_create_players(state, count).await
        }

        ClientMessage::HostTransitionPhase { phase } => {
            if *role != Role::Host {
                return unauthorized("Only host can transition phases");
            }
            handle_host_transition_phase(state, phase).await
        }

        ClientMessage::HostStartRound => {
            if *role != Role::Host {
                return unauthorized("Only host can start rounds");
            }
            handle_host_start_round(state).await
        }

        ClientMessage::HostSelectPrompt { prompt_id } => {
            if *role != Role::Host {
                return unauthorized("Only host can select prompts");
            }
            handle_host_select_prompt(state, prompt_id).await
        }

        ClientMessage::HostEditSubmission {
            submission_id,
            new_text,
        } => {
            if *role != Role::Host {
                return unauthorized("Only host can edit submissions");
            }
            handle_host_edit_submission(state, submission_id, new_text).await
        }

        ClientMessage::HostSetRevealOrder { order } => {
            if *role != Role::Host {
                return unauthorized("Only host can set reveal order");
            }
            handle_host_set_reveal_order(state, order).await
        }

        ClientMessage::HostSetAiSubmission { submission_id } => {
            if *role != Role::Host {
                return unauthorized("Only host can set AI submission");
            }
            handle_host_set_ai_submission(state, submission_id).await
        }

        ClientMessage::HostRevealNext => {
            if *role != Role::Host {
                return unauthorized("Only host can control reveal");
            }
            handle_host_reveal_next(state).await
        }

        ClientMessage::HostRevealPrev => {
            if *role != Role::Host {
                return unauthorized("Only host can control reveal");
            }
            handle_host_reveal_prev(state).await
        }

        ClientMessage::HostResetGame => {
            if *role != Role::Host {
                return unauthorized("Only host can reset game");
            }
            handle_host_reset_game(state).await
        }

        ClientMessage::HostAddPrompt { text } => {
            if *role != Role::Host {
                return unauthorized("Only host can add prompts");
            }
            handle_host_add_prompt(state, text).await
        }

        ClientMessage::HostTogglePanicMode { enabled } => {
            if *role != Role::Host {
                return unauthorized("Only host can toggle panic mode");
            }
            handle_host_toggle_panic_mode(state, enabled).await
        }

        ClientMessage::HostSetManualWinner {
            winner_type,
            submission_id,
        } => {
            if *role != Role::Host {
                return unauthorized("Only host can set manual winners");
            }
            handle_host_set_manual_winner(state, winner_type, submission_id).await
        }

        ClientMessage::HostMarkDuplicate { submission_id } => {
            if *role != Role::Host {
                return unauthorized("Only host can mark duplicates");
            }
            handle_host_mark_duplicate(state, submission_id).await
        }

        ClientMessage::HostExtendTimer { seconds } => {
            if *role != Role::Host {
                return unauthorized("Only host can extend timer");
            }
            handle_host_extend_timer(state, seconds).await
        }

        ClientMessage::RequestTypoCheck { player_token, text } => {
            handle_request_typo_check(state, player_token, text).await
        }

        ClientMessage::UpdateSubmission {
            player_token,
            submission_id,
            new_text,
        } => handle_update_submission(state, player_token, submission_id, new_text).await,
    }
}

fn unauthorized(msg: &str) -> Option<ServerMessage> {
    Some(ServerMessage::Error {
        code: "UNAUTHORIZED".to_string(),
        msg: msg.to_string(),
    })
}

async fn handle_register_player(
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
            // Broadcast updated player status to host (name now set)
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

async fn handle_submit_answer(
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
                    msg: "Invalid player token".to_string(),
                });
            }
        }
    } else {
        None
    };

    match state.submit_answer(&round.id, player_id, text).await {
        Ok(_) => {
            // Broadcast player status update to host (submission status changed)
            broadcast_player_status_to_host(state).await;
            Some(ServerMessage::SubmissionConfirmed)
        }
        Err(e) => Some(ServerMessage::Error {
            code: "SUBMISSION_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_vote(
    state: &Arc<AppState>,
    voter_token: String,
    ai: String,
    funny: String,
    msg_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Vote: AI={}, Funny={}, MsgID={}", ai, funny, msg_id);

    use crate::state::vote::VoteResult;
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
    }
}

async fn handle_submit_prompt(state: &Arc<AppState>, text: String) -> Option<ServerMessage> {
    tracing::info!("Prompt submitted: {}", text);
    let round = state.get_current_round().await?;

    match state
        .add_prompt(&round.id, text, crate::types::PromptSource::Audience)
        .await
    {
        Ok(_) => None,
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_create_players(state: &Arc<AppState>, count: u32) -> Option<ServerMessage> {
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

async fn handle_host_transition_phase(
    state: &Arc<AppState>,
    phase: crate::types::GamePhase,
) -> Option<ServerMessage> {
    tracing::info!("Host transitioning to phase: {:?}", phase);
    match state.transition_phase(phase).await {
        Ok(_) => state.get_game().await.map(|game| {
            let valid_transitions = AppState::get_valid_transitions(&game.phase);
            // Use Phase message for transitions (preserves client state)
            // GameState is only for full game resets
            ServerMessage::Phase {
                phase: game.phase,
                round_no: game.round_no,
                server_now: chrono::Utc::now().to_rfc3339(),
                deadline: game.phase_deadline,
                valid_transitions,
            }
        }),
        Err(e) => Some(ServerMessage::Error {
            code: "TRANSITION_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_start_round(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host starting new round");
    match state.start_round().await {
        Ok(round) => Some(ServerMessage::RoundStarted { round }),
        Err(e) => Some(ServerMessage::Error {
            code: "ROUND_START_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_select_prompt(
    state: &Arc<AppState>,
    prompt_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host selecting prompt: {}", prompt_id);
    let round = state.get_current_round().await?;

    match state.select_prompt(&round.id, &prompt_id).await {
        Ok(_) => {
            let updated_round = state.get_current_round().await?;
            updated_round
                .selected_prompt
                .map(|prompt| ServerMessage::PromptSelected { prompt })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_SELECT_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_edit_submission(
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

async fn handle_host_set_reveal_order(
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

async fn handle_host_set_ai_submission(
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

async fn handle_host_reveal_next(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host advancing reveal");
    let round = state.get_current_round().await?;

    match state.reveal_next(&round.id).await {
        Ok(reveal_index) => {
            // Get the submission at the current reveal index
            let submission = state.get_current_reveal_submission(&round.id).await;
            let submission_info = submission.map(|s| crate::protocol::SubmissionInfo::from(&s));

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

async fn handle_host_reveal_prev(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host going back in reveal");
    let round = state.get_current_round().await?;

    match state.reveal_prev(&round.id).await {
        Ok(reveal_index) => {
            // Get the submission at the current reveal index
            let submission = state.get_current_reveal_submission(&round.id).await;
            let submission_info = submission.map(|s| crate::protocol::SubmissionInfo::from(&s));

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

async fn handle_host_reset_game(state: &Arc<AppState>) -> Option<ServerMessage> {
    tracing::info!("Host resetting game");
    state.reset_game().await;

    // Return game state after reset
    let game = state.get_game().await?;
    let valid_transitions = AppState::get_valid_transitions(&game.phase);
    Some(ServerMessage::GameState {
        game,
        valid_transitions,
    })
}

async fn handle_host_add_prompt(state: &Arc<AppState>, text: String) -> Option<ServerMessage> {
    tracing::info!("Host adding prompt: {}", text);
    let round = state.get_current_round().await?;

    match state
        .add_prompt(&round.id, text, crate::types::PromptSource::Host)
        .await
    {
        Ok(prompt) => {
            // Auto-select the prompt when added by host
            let prompt_id = prompt.id.clone();
            if let Err(e) = state.select_prompt(&round.id, &prompt_id).await {
                tracing::warn!("Failed to auto-select prompt: {}", e);
            }
            Some(ServerMessage::PromptSelected { prompt })
        }
        Err(e) => Some(ServerMessage::Error {
            code: "PROMPT_ADD_FAILED".to_string(),
            msg: e,
        }),
    }
}

async fn handle_host_toggle_panic_mode(
    state: &Arc<AppState>,
    enabled: bool,
) -> Option<ServerMessage> {
    tracing::info!("Host toggling panic mode: {}", enabled);
    state.set_panic_mode(enabled).await;
    Some(ServerMessage::PanicModeUpdate { enabled })
}

async fn handle_host_set_manual_winner(
    state: &Arc<AppState>,
    winner_type: crate::protocol::ManualWinnerType,
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
        crate::protocol::ManualWinnerType::Ai => {
            state.set_manual_ai_winner(&round.id, submission_id).await
        }
        crate::protocol::ManualWinnerType::Funny => {
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

async fn handle_host_mark_duplicate(
    state: &Arc<AppState>,
    submission_id: String,
) -> Option<ServerMessage> {
    tracing::info!("Host marking submission as duplicate: {}", submission_id);

    match state.mark_submission_duplicate(&submission_id).await {
        Ok(player_id) => {
            // If it was a player submission, notify the player via broadcast
            // (players filter by their own ID)
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

async fn handle_host_extend_timer(state: &Arc<AppState>, seconds: u32) -> Option<ServerMessage> {
    tracing::info!("Host extending timer by {} seconds", seconds);

    match state.extend_deadline(seconds).await {
        Ok(new_deadline) => {
            let server_now = chrono::Utc::now().to_rfc3339();
            // Broadcast to all clients
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

async fn handle_request_typo_check(
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
                msg: "Invalid player token".to_string(),
            });
        }
    };

    // Set player status to checking typos
    state
        .set_player_status(
            &player.id,
            crate::protocol::PlayerSubmissionStatus::CheckingTypos,
        )
        .await;

    // Broadcast player status update to host
    broadcast_player_status_to_host(state).await;

    // Check if we have an LLM provider
    let llm = match &state.llm {
        Some(llm) => llm,
        None => {
            tracing::warn!("No LLM provider available for typo check, returning original");
            // Clear the checking status since we're done
            state
                .set_player_status(
                    &player.id,
                    crate::protocol::PlayerSubmissionStatus::NotSubmitted,
                )
                .await;
            broadcast_player_status_to_host(state).await;
            return Some(ServerMessage::TypoCheckResult {
                original: text.clone(),
                corrected: text,
                has_changes: false,
            });
        }
    };

    // Use the first available provider for typo checking
    // In the future we could make this configurable
    let providers = &llm.providers;
    if providers.is_empty() {
        tracing::warn!("No LLM providers configured for typo check");
        state
            .set_player_status(
                &player.id,
                crate::protocol::PlayerSubmissionStatus::NotSubmitted,
            )
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
        .set_player_status(
            &player.id,
            crate::protocol::PlayerSubmissionStatus::NotSubmitted,
        )
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

async fn handle_update_submission(
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
                msg: "Invalid player token".to_string(),
            });
        }
    };

    // Update the submission
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

/// Broadcast current player status to host
async fn broadcast_player_status_to_host(state: &Arc<AppState>) {
    let players = state.get_all_player_status().await;
    state.broadcast_to_host(ServerMessage::HostPlayerStatus { players });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GamePhase;

    #[tokio::test]
    async fn test_unauthorized_host_command() {
        let state = Arc::new(AppState::new());
        let role = Role::Audience;

        let result =
            handle_message(ClientMessage::HostCreatePlayers { count: 3 }, &role, &state).await;

        assert!(result.is_some());
        if let Some(ServerMessage::Error { code, .. }) = result {
            assert_eq!(code, "UNAUTHORIZED");
        }
    }

    #[tokio::test]
    async fn test_host_create_players() {
        let state = Arc::new(AppState::new());
        let role = Role::Host;

        let result =
            handle_message(ClientMessage::HostCreatePlayers { count: 2 }, &role, &state).await;

        assert!(result.is_some());
        if let Some(ServerMessage::PlayersCreated { players }) = result {
            assert_eq!(players.len(), 2);
        } else {
            panic!("Expected PlayersCreated message");
        }
    }

    #[tokio::test]
    async fn test_phase_transition() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Host;

        // Test valid transition: Lobby -> PromptSelection
        let result = handle_message(
            ClientMessage::HostTransitionPhase {
                phase: GamePhase::PromptSelection,
            },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
        // Phase transitions now return Phase message instead of GameState
        if let Some(ServerMessage::Phase { phase, .. }) = result {
            assert_eq!(phase, GamePhase::PromptSelection);
        } else {
            panic!("Expected Phase message");
        }
    }

    #[tokio::test]
    async fn test_reset_game() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Host;

        // Create some players
        handle_message(ClientMessage::HostCreatePlayers { count: 3 }, &role, &state).await;

        // Transition to a different phase
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .ok();

        // Verify state is not empty
        let players = state.players.read().await;
        assert_eq!(players.len(), 3);
        drop(players);

        let game_before = state.get_game().await.unwrap();
        assert_eq!(game_before.phase, GamePhase::PromptSelection);

        // Reset the game
        let result = handle_message(ClientMessage::HostResetGame, &role, &state).await;

        // Verify response
        assert!(result.is_some());
        if let Some(ServerMessage::GameState { game, .. }) = result {
            assert_eq!(game.phase, GamePhase::Lobby);
            assert_eq!(game.round_no, 0);
            assert_eq!(game.current_round_id, None);
        } else {
            panic!("Expected GameState message");
        }

        // Verify all state is cleared
        let players = state.players.read().await;
        assert_eq!(players.len(), 0);
        drop(players);

        let rounds = state.rounds.read().await;
        assert_eq!(rounds.len(), 0);
        drop(rounds);

        let submissions = state.submissions.read().await;
        assert_eq!(submissions.len(), 0);
        drop(submissions);

        let votes = state.votes.read().await;
        assert_eq!(votes.len(), 0);
        drop(votes);

        let scores = state.scores.read().await;
        assert_eq!(scores.len(), 0);
    }

    #[tokio::test]
    async fn test_reset_game_requires_host() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Audience;

        let result = handle_message(ClientMessage::HostResetGame, &role, &state).await;

        assert!(result.is_some());
        if let Some(ServerMessage::Error { code, .. }) = result {
            assert_eq!(code, "UNAUTHORIZED");
        } else {
            panic!("Expected Error message");
        }
    }

    #[tokio::test]
    async fn test_panic_mode_toggle() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Host;

        // Enable panic mode
        let result = handle_message(
            ClientMessage::HostTogglePanicMode { enabled: true },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
        if let Some(ServerMessage::PanicModeUpdate { enabled }) = result {
            assert!(enabled);
        } else {
            panic!("Expected PanicModeUpdate message");
        }

        // Verify state
        assert!(state.is_panic_mode().await);

        // Disable panic mode
        let result = handle_message(
            ClientMessage::HostTogglePanicMode { enabled: false },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
        if let Some(ServerMessage::PanicModeUpdate { enabled }) = result {
            assert!(!enabled);
        }

        assert!(!state.is_panic_mode().await);
    }

    #[tokio::test]
    async fn test_panic_mode_blocks_votes() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        state.start_round().await.ok();

        // Enable panic mode
        state.set_panic_mode(true).await;

        // Try to vote
        use crate::state::vote::VoteResult;
        let result = state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::PanicModeActive);
    }

    #[tokio::test]
    async fn test_panic_mode_requires_host() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Audience;

        let result = handle_message(
            ClientMessage::HostTogglePanicMode { enabled: true },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
        if let Some(ServerMessage::Error { code, .. }) = result {
            assert_eq!(code, "UNAUTHORIZED");
        } else {
            panic!("Expected Error message");
        }
    }

    #[tokio::test]
    async fn test_mark_duplicate_requires_host() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Audience;

        let result = handle_message(
            ClientMessage::HostMarkDuplicate {
                submission_id: "sub1".to_string(),
            },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
        if let Some(ServerMessage::Error { code, .. }) = result {
            assert_eq!(code, "UNAUTHORIZED");
        } else {
            panic!("Expected Error message");
        }
    }

    #[tokio::test]
    async fn test_mark_duplicate_success() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let round = state.start_round().await.unwrap();
        let role = Role::Host;

        // Create a player submission
        let player = state.create_player().await;
        let sub = state
            .submit_answer(&round.id, Some(player.id.clone()), "Test".to_string())
            .await
            .unwrap();

        // Mark as duplicate
        let result = handle_message(
            ClientMessage::HostMarkDuplicate {
                submission_id: sub.id.clone(),
            },
            &role,
            &state,
        )
        .await;

        // Should return None (success, broadcast handled separately)
        assert!(result.is_none());

        // Verify submission is removed
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 0);
    }

    #[tokio::test]
    async fn test_mark_duplicate_nonexistent() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Host;

        let result = handle_message(
            ClientMessage::HostMarkDuplicate {
                submission_id: "nonexistent".to_string(),
            },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
        if let Some(ServerMessage::Error { code, .. }) = result {
            assert_eq!(code, "MARK_DUPLICATE_FAILED");
        } else {
            panic!("Expected Error message");
        }
    }
}
