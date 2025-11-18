mod game;
mod player;
mod round;
mod score;
mod submission;
mod vote;

use crate::protocol::ServerMessage;
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub game: Arc<RwLock<Option<Game>>>,
    pub rounds: Arc<RwLock<HashMap<RoundId, Round>>>,
    pub submissions: Arc<RwLock<HashMap<SubmissionId, Submission>>>,
    pub votes: Arc<RwLock<HashMap<VoteId, Vote>>>,
    pub players: Arc<RwLock<HashMap<PlayerId, Player>>>,
    pub scores: Arc<RwLock<Vec<Score>>>,
    /// Broadcast channel for sending messages to Beamer clients
    pub beamer_broadcast: broadcast::Sender<ServerMessage>,
}

impl AppState {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self {
            game: Arc::new(RwLock::new(None)),
            rounds: Arc::new(RwLock::new(HashMap::new())),
            submissions: Arc::new(RwLock::new(HashMap::new())),
            votes: Arc::new(RwLock::new(HashMap::new())),
            players: Arc::new(RwLock::new(HashMap::new())),
            scores: Arc::new(RwLock::new(Vec::new())),
            beamer_broadcast: tx,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_game() {
        let state = AppState::new();
        let game = state.create_game().await;

        assert_eq!(game.phase, GamePhase::Lobby);
        assert_eq!(game.round_no, 0);
        assert!(state.get_game().await.is_some());
    }

    #[tokio::test]
    async fn test_create_player() {
        let state = AppState::new();
        let player = state.create_player().await;

        assert!(player.display_name.is_none());
        assert!(!player.token.is_empty());
        assert!(state.get_player_by_token(&player.token).await.is_some());
    }

    #[tokio::test]
    async fn test_register_player() {
        let state = AppState::new();
        let player = state.create_player().await;
        let token = player.token.clone();

        let result = state
            .register_player(&token, "TestPlayer".to_string())
            .await;
        assert!(result.is_ok());

        let registered = result.unwrap();
        assert_eq!(registered.display_name, Some("TestPlayer".to_string()));
    }

    #[tokio::test]
    async fn test_round_lifecycle() {
        let state = AppState::new();
        state.create_game().await;

        let round = state.start_round().await.unwrap();
        assert_eq!(round.number, 1);
        assert_eq!(round.state, RoundState::Setup);

        let current = state.get_current_round().await;
        assert!(current.is_some());
        assert_eq!(current.unwrap().id, round.id);
    }

    // GamePhase validation tests

    #[tokio::test]
    async fn test_valid_game_phase_transitions() {
        let state = AppState::new();
        state.create_game().await;

        // Lobby -> PromptSelection
        assert!(state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .is_ok());

        // PromptSelection -> Writing requires prompt selected
        // Skip for now, test separately

        // Test panic mode: any phase -> Intermission
        assert!(state
            .transition_phase(GamePhase::Intermission)
            .await
            .is_ok());

        // Intermission -> any phase
        assert!(state.transition_phase(GamePhase::Lobby).await.is_ok());

        // Test hard stop: any phase -> Ended
        assert!(state.transition_phase(GamePhase::Ended).await.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_game_phase_transitions() {
        let state = AppState::new();
        state.create_game().await;

        // Can't go from Lobby to Writing directly
        let result = state.transition_phase(GamePhase::Writing).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid phase transition"));

        // Can't go from Lobby to Voting
        let result = state.transition_phase(GamePhase::Voting).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_game_phase_preconditions() {
        let state = AppState::new();
        state.create_game().await;

        // Try to go to Writing without a round
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();
        let result = state.transition_phase(GamePhase::Writing).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Writing phase requires an active round"));
    }

    #[tokio::test]
    async fn test_writing_phase_requires_prompt() {
        let state = AppState::new();
        state.create_game().await;
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();

        let round = state.start_round().await.unwrap();

        // Try to transition to Writing without selected prompt
        let result = state.transition_phase(GamePhase::Writing).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("selected prompt"));

        // Add and select a prompt
        let prompt = state
            .add_prompt(&round.id, "Test prompt".to_string(), PromptSource::Host)
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();

        // Now transition should work
        assert!(state.transition_phase(GamePhase::Writing).await.is_ok());
    }

    #[tokio::test]
    async fn test_reveal_phase_requires_submissions() {
        let state = AppState::new();
        state.create_game().await;
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();

        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt(&round.id, "Test prompt".to_string(), PromptSource::Host)
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();

        // Try to go to Reveal without submissions
        let result = state.transition_phase(GamePhase::Reveal).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one submission"));
    }

    #[tokio::test]
    async fn test_voting_phase_requires_reveal_order() {
        let state = AppState::new();
        state.create_game().await;
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();

        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt(&round.id, "Test prompt".to_string(), PromptSource::Host)
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();

        // Add a submission
        let player = state.create_player().await;
        state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Test answer".to_string(),
            )
            .await
            .unwrap();

        state.transition_phase(GamePhase::Reveal).await.unwrap();

        // Try to go to Voting without reveal order
        let result = state.transition_phase(GamePhase::Voting).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("reveal order"));
    }

    // RoundState validation tests

    #[tokio::test]
    async fn test_valid_round_state_transitions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add prompt candidates and select one
        let prompt = state
            .add_prompt(&round.id, "Test prompt".to_string(), PromptSource::Host)
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();

        // Should now be in Collecting state
        let current = state.get_current_round().await.unwrap();
        assert_eq!(current.state, RoundState::Collecting);
    }

    #[tokio::test]
    async fn test_invalid_round_state_transitions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Can't go from Setup to Revealing
        let result = state
            .transition_round_state(&round.id, RoundState::Revealing)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid round state transition"));
    }

    #[tokio::test]
    async fn test_round_state_preconditions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Can't transition to Collecting without selected prompt
        let result = state
            .transition_round_state(&round.id, RoundState::Collecting)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("selected prompt"));
    }

    #[tokio::test]
    async fn test_revealing_requires_submissions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        let prompt = state
            .add_prompt(&round.id, "Test prompt".to_string(), PromptSource::Host)
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();

        // Try to transition to Revealing without submissions
        let result = state
            .transition_round_state(&round.id, RoundState::Revealing)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one submission"));
    }

    #[tokio::test]
    async fn test_select_prompt_validates_state() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        let prompt = state
            .add_prompt(&round.id, "Test prompt".to_string(), PromptSource::Host)
            .await
            .unwrap();

        // First selection should work
        assert!(state.select_prompt(&round.id, &prompt.id).await.is_ok());

        // Try to select again when not in Setup
        let prompt2 = state
            .add_prompt(&round.id, "Test prompt 2".to_string(), PromptSource::Host)
            .await
            .unwrap();

        let result = state.select_prompt(&round.id, &prompt2.id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Setup state"));
    }

    #[tokio::test]
    async fn test_start_round_validates_phase() {
        let state = AppState::new();
        state.create_game().await;

        // First round should work in Lobby
        assert!(state.start_round().await.is_ok());

        // Transition to Writing phase
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();
        let round = state.get_current_round().await.unwrap();
        let prompt = state
            .add_prompt(&round.id, "Test".to_string(), PromptSource::Host)
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();

        // Should not be able to start round in Writing phase
        let result = state.start_round().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot start round"));
    }

    #[tokio::test]
    async fn test_start_round_requires_closed_previous_round() {
        let state = AppState::new();
        state.create_game().await;

        let _round = state.start_round().await.unwrap();

        // Try to start another round while first is still in Setup
        let result = state.start_round().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("current round is in"));
    }

    #[tokio::test]
    async fn test_set_reveal_order_validates_submissions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Empty order should fail
        let result = state.set_reveal_order(&round.id, vec![]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be empty"));

        // Non-existent submission should fail
        let result = state
            .set_reveal_order(&round.id, vec!["fake_id".to_string()])
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));

        // Create a submission
        let player = state.create_player().await;
        let submission = state
            .submit_answer(&round.id, Some(player.id.clone()), "Test".to_string())
            .await
            .unwrap();

        // Valid order should work
        let result = state
            .set_reveal_order(&round.id, vec![submission.id.clone()])
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_reveal_order_validates_round_ownership() {
        let state = AppState::new();
        state.create_game().await;

        // Create first round
        let round1 = state.start_round().await.unwrap();

        // Close first round by transitioning through phases
        let mut rounds = state.rounds.write().await;
        if let Some(r) = rounds.get_mut(&round1.id) {
            r.state = RoundState::Closed;
        }
        drop(rounds);

        // Transition to Results phase first (valid path)
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();
        let round2 = state.start_round().await.unwrap();

        // Create submission in round 2
        let player = state.create_player().await;
        let submission = state
            .submit_answer(&round2.id, Some(player.id), "Test".to_string())
            .await
            .unwrap();

        // Try to use round2's submission in round1's reveal order
        let result = state
            .set_reveal_order(&round1.id, vec![submission.id])
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not belong to round"));
    }
}
