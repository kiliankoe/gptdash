//! WebSocket message dispatch
//!
//! This module provides the main entry point for handling client messages.
//! Authorization is checked here, then dispatched to role-specific handler modules.

use crate::protocol::{ClientMessage, ServerMessage};
use crate::state::AppState;
use crate::types::Role;
use std::sync::Arc;

use super::{audience, host, player};

/// Macro to check host authorization and return early if unauthorized
macro_rules! check_host {
    ($role:expr, $action:expr) => {
        if *$role != Role::Host {
            return Some(ServerMessage::Error {
                code: "UNAUTHORIZED".to_string(),
                msg: format!("Only host can {}", $action),
            });
        }
    };
}

/// Handle client messages and return optional response
pub async fn handle_message(
    msg: ClientMessage,
    role: &Role,
    state: &Arc<AppState>,
) -> Option<ServerMessage> {
    match msg {
        // Connection messages
        ClientMessage::Join { room_token } => {
            tracing::info!("Join request with token: {}", room_token);
            None
        }

        ClientMessage::AckNeeded {
            last_seen_server_seq,
        } => {
            tracing::info!("Ack needed: seq={}", last_seen_server_seq);
            None
        }

        // Player messages
        ClientMessage::RegisterPlayer {
            player_token,
            display_name,
        } => player::handle_register_player(state, player_token, display_name).await,

        ClientMessage::SubmitAnswer { player_token, text } => {
            player::handle_submit_answer(state, player_token, text).await
        }

        ClientMessage::RequestTypoCheck { player_token, text } => {
            player::handle_request_typo_check(state, player_token, text).await
        }

        ClientMessage::UpdateSubmission {
            player_token,
            submission_id,
            new_text,
        } => player::handle_update_submission(state, player_token, submission_id, new_text).await,

        // Audience messages
        ClientMessage::Vote {
            voter_token,
            ai,
            funny,
            msg_id,
        } => audience::handle_vote(state, voter_token, ai, funny, msg_id).await,

        ClientMessage::SubmitPrompt { voter_token, text } => {
            audience::handle_submit_prompt(state, voter_token, text).await
        }

        // Host-only commands (authorization checked before dispatch)
        ClientMessage::HostCreatePlayers { count } => {
            check_host!(role, "create players");
            host::handle_create_players(state, count).await
        }

        ClientMessage::HostTransitionPhase { phase } => {
            check_host!(role, "transition phases");
            host::handle_transition_phase(state, phase).await
        }

        ClientMessage::HostStartRound => {
            check_host!(role, "start rounds");
            host::handle_start_round(state).await
        }

        ClientMessage::HostSelectPrompt { prompt_id } => {
            check_host!(role, "select prompts");
            host::handle_select_prompt(state, prompt_id).await
        }

        ClientMessage::HostEditSubmission {
            submission_id,
            new_text,
        } => {
            check_host!(role, "edit submissions");
            host::handle_edit_submission(state, submission_id, new_text).await
        }

        ClientMessage::HostSetRevealOrder { order } => {
            check_host!(role, "set reveal order");
            host::handle_set_reveal_order(state, order).await
        }

        ClientMessage::HostSetAiSubmission { submission_id } => {
            check_host!(role, "set AI submission");
            host::handle_set_ai_submission(state, submission_id).await
        }

        ClientMessage::HostRevealNext => {
            check_host!(role, "control reveal");
            host::handle_reveal_next(state).await
        }

        ClientMessage::HostRevealPrev => {
            check_host!(role, "control reveal");
            host::handle_reveal_prev(state).await
        }

        ClientMessage::HostResetGame => {
            check_host!(role, "reset game");
            host::handle_reset_game(state).await
        }

        ClientMessage::HostAddPrompt { text } => {
            check_host!(role, "add prompts");
            host::handle_add_prompt(state, text).await
        }

        ClientMessage::HostTogglePanicMode { enabled } => {
            check_host!(role, "toggle panic mode");
            host::handle_toggle_panic_mode(state, enabled).await
        }

        ClientMessage::HostSetManualWinner {
            winner_type,
            submission_id,
        } => {
            check_host!(role, "set manual winners");
            host::handle_set_manual_winner(state, winner_type, submission_id).await
        }

        ClientMessage::HostMarkDuplicate { submission_id } => {
            check_host!(role, "mark duplicates");
            host::handle_mark_duplicate(state, submission_id).await
        }

        ClientMessage::HostExtendTimer { seconds } => {
            check_host!(role, "extend timer");
            host::handle_extend_timer(state, seconds).await
        }

        ClientMessage::HostRegenerateAi => {
            check_host!(role, "regenerate AI");
            host::handle_regenerate_ai(state).await
        }

        ClientMessage::HostWriteAiSubmission { text } => {
            check_host!(role, "write AI submissions");
            host::handle_write_ai_submission(state, text).await
        }

        ClientMessage::HostShadowbanAudience { voter_id } => {
            check_host!(role, "shadowban audience members");
            host::handle_shadowban_audience(state, voter_id).await
        }

        ClientMessage::HostRemovePlayer { player_id } => {
            check_host!(role, "remove players");
            host::handle_remove_player(state, player_id).await
        }
    }
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

        let result = handle_message(
            ClientMessage::HostTransitionPhase {
                phase: GamePhase::PromptSelection,
            },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
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

    #[tokio::test]
    async fn test_shadowban_requires_host() {
        let state = Arc::new(AppState::new());
        let role = Role::Audience;

        let result = handle_message(
            ClientMessage::HostShadowbanAudience {
                voter_id: "voter1".to_string(),
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
    async fn test_shadowban_success() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Host;

        // Verify not shadowbanned initially
        assert!(!state.is_shadowbanned("voter1").await);

        // Shadowban the voter
        let result = handle_message(
            ClientMessage::HostShadowbanAudience {
                voter_id: "voter1".to_string(),
            },
            &role,
            &state,
        )
        .await;

        // Should return None (silent success)
        assert!(result.is_none());

        // Verify voter is now shadowbanned
        assert!(state.is_shadowbanned("voter1").await);
    }

    #[tokio::test]
    async fn test_submit_prompt_with_voter_token() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let _round = state.start_round().await.unwrap();
        let role = Role::Audience;

        let result = handle_message(
            ClientMessage::SubmitPrompt {
                voter_token: Some("voter123".to_string()),
                text: "My prompt suggestion".to_string(),
            },
            &role,
            &state,
        )
        .await;

        // Should return None (silent success)
        assert!(result.is_none());

        // Verify prompt was added with submitter_id
        let round = state.get_current_round().await.unwrap();
        let rounds = state.rounds.read().await;
        let round_data = rounds.get(&round.id).unwrap();
        assert_eq!(round_data.prompt_candidates.len(), 1);
        assert_eq!(
            round_data.prompt_candidates[0].submitter_id,
            Some("voter123".to_string())
        );
    }

    #[tokio::test]
    async fn test_shadowbanned_user_prompt_silently_ignored() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let _round = state.start_round().await.unwrap();
        let role = Role::Audience;

        // First, shadowban the voter
        state.shadowban_audience("spammer".to_string()).await;

        // Try to submit a prompt as shadowbanned user
        let result = handle_message(
            ClientMessage::SubmitPrompt {
                voter_token: Some("spammer".to_string()),
                text: "Spam prompt".to_string(),
            },
            &role,
            &state,
        )
        .await;

        // Should return None (user thinks it succeeded)
        assert!(result.is_none());

        // But prompt was NOT actually added
        let round = state.get_current_round().await.unwrap();
        let rounds = state.rounds.read().await;
        let round_data = rounds.get(&round.id).unwrap();
        assert_eq!(round_data.prompt_candidates.len(), 0);
    }

    // Remove player handler tests

    #[tokio::test]
    async fn test_remove_player_requires_host() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let player = state.create_player().await;
        let role = Role::Audience;

        let result = handle_message(
            ClientMessage::HostRemovePlayer {
                player_id: player.id.clone(),
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

        // Player should still exist
        assert!(state.get_player_by_token(&player.token).await.is_some());
    }

    #[tokio::test]
    async fn test_remove_player_success() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let player = state.create_player().await;
        let role = Role::Host;

        // Verify player exists
        assert!(state.get_player_by_token(&player.token).await.is_some());

        let result = handle_message(
            ClientMessage::HostRemovePlayer {
                player_id: player.id.clone(),
            },
            &role,
            &state,
        )
        .await;

        // Should return None (success, broadcast handled separately)
        assert!(result.is_none());

        // Player should be gone
        assert!(state.get_player_by_token(&player.token).await.is_none());
    }

    #[tokio::test]
    async fn test_remove_player_not_found() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let role = Role::Host;

        let result = handle_message(
            ClientMessage::HostRemovePlayer {
                player_id: "nonexistent".to_string(),
            },
            &role,
            &state,
        )
        .await;

        assert!(result.is_some());
        if let Some(ServerMessage::Error { code, .. }) = result {
            assert_eq!(code, "REMOVE_PLAYER_FAILED");
        } else {
            panic!("Expected Error message");
        }
    }

    #[tokio::test]
    async fn test_remove_player_with_submission() {
        let state = Arc::new(AppState::new());
        state.create_game().await;
        let round = state.start_round().await.unwrap();
        let player = state.create_player().await;
        let role = Role::Host;

        // Create submission for player
        state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Test answer".to_string(),
            )
            .await
            .unwrap();

        // Verify submission exists
        assert_eq!(state.get_submissions(&round.id).await.len(), 1);

        // Remove player
        let result = handle_message(
            ClientMessage::HostRemovePlayer {
                player_id: player.id.clone(),
            },
            &role,
            &state,
        )
        .await;

        // Should succeed
        assert!(result.is_none());

        // Player and submission should be gone
        assert!(state.get_player_by_token(&player.token).await.is_none());
        assert_eq!(state.get_submissions(&round.id).await.len(), 0);
    }
}
