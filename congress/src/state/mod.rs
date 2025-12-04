pub mod export;
mod game;
mod player;
mod round;
mod score;
mod submission;
pub mod vote;

use crate::llm::{LlmConfig, LlmManager};
use crate::protocol::{PlayerSubmissionStatus, ServerMessage};
use crate::types::*;
use export::GameStateExport;
use std::collections::{HashMap, HashSet};
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
    /// Processed vote msg_ids per voter for idempotency (voter_id -> msg_id)
    pub processed_vote_msg_ids: Arc<RwLock<HashMap<VoterId, String>>>,
    /// Player submission status tracking (player_id -> status)
    pub player_status: Arc<RwLock<HashMap<PlayerId, PlayerSubmissionStatus>>>,
    /// Shadowbanned audience member IDs (their prompts are silently ignored)
    pub shadowbanned_audience: Arc<RwLock<HashSet<VoterId>>>,
    /// LLM manager for generating AI answers
    pub llm: Option<Arc<LlmManager>>,
    /// LLM configuration (timeout, max tokens, etc.)
    pub llm_config: LlmConfig,
    /// Broadcast channel for sending messages to all clients
    pub broadcast: broadcast::Sender<ServerMessage>,
    /// Broadcast channel for sending messages to Host clients only
    pub host_broadcast: broadcast::Sender<ServerMessage>,
    /// Broadcast channel for sending messages to Beamer clients only
    pub beamer_broadcast: broadcast::Sender<ServerMessage>,
}

impl AppState {
    pub fn new() -> Self {
        Self::new_with_llm(None, LlmConfig::default())
    }

    pub fn new_with_llm(llm: Option<LlmManager>, llm_config: LlmConfig) -> Self {
        let (broadcast_tx, _rx) = broadcast::channel(100);
        let (host_tx, _rx) = broadcast::channel(100);
        let (beamer_tx, _rx) = broadcast::channel(100);
        Self {
            game: Arc::new(RwLock::new(None)),
            rounds: Arc::new(RwLock::new(HashMap::new())),
            submissions: Arc::new(RwLock::new(HashMap::new())),
            votes: Arc::new(RwLock::new(HashMap::new())),
            players: Arc::new(RwLock::new(HashMap::new())),
            scores: Arc::new(RwLock::new(Vec::new())),
            processed_vote_msg_ids: Arc::new(RwLock::new(HashMap::new())),
            player_status: Arc::new(RwLock::new(HashMap::new())),
            shadowbanned_audience: Arc::new(RwLock::new(HashSet::new())),
            llm: llm.map(Arc::new),
            llm_config,
            broadcast: broadcast_tx,
            host_broadcast: host_tx,
            beamer_broadcast: beamer_tx,
        }
    }

    /// Broadcast a message to all connected clients
    pub fn broadcast_to_all(&self, msg: ServerMessage) {
        let _ = self.broadcast.send(msg);
    }

    /// Broadcast a message to host clients only
    pub fn broadcast_to_host(&self, msg: ServerMessage) {
        let _ = self.host_broadcast.send(msg);
    }

    /// Check if a voter is shadowbanned
    pub async fn is_shadowbanned(&self, voter_id: &str) -> bool {
        self.shadowbanned_audience.read().await.contains(voter_id)
    }

    /// Shadowban an audience member
    pub async fn shadowban_audience(&self, voter_id: String) {
        self.shadowbanned_audience.write().await.insert(voter_id);
    }

    /// Get all shadowbanned audience member IDs
    pub async fn get_shadowbanned_audience(&self) -> Vec<String> {
        self.shadowbanned_audience
            .read()
            .await
            .iter()
            .cloned()
            .collect()
    }

    /// Broadcast prompt candidates to host (filtered by shadowban status)
    pub async fn broadcast_prompts_to_host(&self, round_id: &str) {
        let rounds = self.rounds.read().await;
        let round = match rounds.get(round_id) {
            Some(r) => r,
            None => return,
        };

        let shadowbanned = self.shadowbanned_audience.read().await;

        // Filter out prompts from shadowbanned users
        let prompts: Vec<crate::protocol::HostPromptInfo> = round
            .prompt_candidates
            .iter()
            .filter(|p| {
                // Keep prompts from non-shadowbanned users or prompts without submitter_id
                match &p.submitter_id {
                    Some(id) => !shadowbanned.contains(id),
                    None => true,
                }
            })
            .map(|p| crate::protocol::HostPromptInfo {
                id: p.id.clone(),
                text: p.text.clone(),
                image_url: p.image_url.clone(),
                source: p.source.clone(),
                submitter_id: p.submitter_id.clone(),
            })
            .collect();

        self.broadcast_to_host(ServerMessage::HostPrompts { prompts });
    }

    /// Export the entire game state as a serializable snapshot.
    ///
    /// Acquires all locks to ensure a consistent snapshot.
    pub async fn export_state(&self) -> GameStateExport {
        // Acquire all locks to get a consistent snapshot
        let game = self.game.read().await.clone();
        let rounds = self.rounds.read().await.clone();
        let submissions = self.submissions.read().await.clone();
        let votes = self.votes.read().await.clone();
        let players = self.players.read().await.clone();
        let scores = self.scores.read().await.clone();
        let processed_vote_msg_ids = self.processed_vote_msg_ids.read().await.clone();
        let player_status = self.player_status.read().await.clone();
        let shadowbanned_audience = self.shadowbanned_audience.read().await.clone();

        GameStateExport::new(
            game,
            rounds,
            submissions,
            votes,
            players,
            scores,
            processed_vote_msg_ids,
            player_status,
            shadowbanned_audience,
        )
    }

    /// Import a state snapshot, replacing all current state.
    ///
    /// This validates the import first, then atomically replaces all state.
    /// After import, broadcasts a full state refresh to all connected clients.
    pub async fn import_state(&self, export: GameStateExport) -> Result<(), String> {
        // Validate before importing
        export.validate()?;

        // Acquire all write locks and replace state
        *self.game.write().await = export.game.clone();
        *self.rounds.write().await = export.rounds;
        *self.submissions.write().await = export.submissions;
        *self.votes.write().await = export.votes;
        *self.players.write().await = export.players;
        *self.scores.write().await = export.scores;
        *self.processed_vote_msg_ids.write().await = export.processed_vote_msg_ids;
        *self.player_status.write().await = export.player_status;
        *self.shadowbanned_audience.write().await = export.shadowbanned_audience;

        // Broadcast state refresh to all clients
        if let Some(ref game) = export.game {
            let valid_transitions = Self::get_valid_transitions(&game.phase);
            self.broadcast_to_all(ServerMessage::GameState {
                game: game.clone(),
                valid_transitions,
            });
        }

        tracing::info!("State imported successfully");
        Ok(())
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
            .add_prompt(
                &round.id,
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
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
            .add_prompt(
                &round.id,
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
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
    async fn test_reveal_auto_populates_reveal_order() {
        let state = AppState::new();
        state.create_game().await;
        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();

        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt(
                &round.id,
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();

        // Add a submission
        let player = state.create_player().await;
        let sub = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Test answer".to_string(),
            )
            .await
            .unwrap();

        // Transition to Reveal should auto-populate reveal_order
        state.transition_phase(GamePhase::Reveal).await.unwrap();

        // Check reveal_order was auto-populated
        let current_round = state.get_current_round().await.unwrap();
        assert!(!current_round.reveal_order.is_empty());
        assert!(current_round.reveal_order.contains(&sub.id));
    }

    // RoundState validation tests

    #[tokio::test]
    async fn test_valid_round_state_transitions() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add prompt candidates and select one
        let prompt = state
            .add_prompt(
                &round.id,
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
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
            .add_prompt(
                &round.id,
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
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
            .add_prompt(
                &round.id,
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();

        // First selection should work
        assert!(state.select_prompt(&round.id, &prompt.id).await.is_ok());

        // Try to select again when not in Setup
        let prompt2 = state
            .add_prompt(
                &round.id,
                Some("Test prompt 2".to_string()),
                None,
                PromptSource::Host,
                None,
            )
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
            .add_prompt(
                &round.id,
                Some("Test".to_string()),
                None,
                PromptSource::Host,
                None,
            )
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

    #[tokio::test]
    async fn test_results_phase_requires_ai_submission() {
        let state = AppState::new();
        state.create_game().await;

        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();
        let round = state.start_round().await.unwrap();

        // Add and select prompt
        let prompt = state
            .add_prompt(
                &round.id,
                Some("Test prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();

        // Add submissions
        let player = state.create_player().await;
        let sub1 = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Player answer".to_string(),
            )
            .await
            .unwrap();
        let _sub2 = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();

        // Set reveal order
        state
            .set_reveal_order(&round.id, vec![sub1.id.clone()])
            .await
            .unwrap();

        // Progress through phases
        state.transition_phase(GamePhase::Writing).await.unwrap();
        state.transition_phase(GamePhase::Reveal).await.unwrap();
        state.transition_phase(GamePhase::Voting).await.unwrap();

        // Try to go to RESULTS without setting AI submission
        let result = state.transition_phase(GamePhase::Results).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AI submission to be set"));
    }

    #[tokio::test]
    async fn test_scoring_is_idempotent() {
        let state = AppState::new();
        state.create_game().await;

        state
            .transition_phase(GamePhase::PromptSelection)
            .await
            .unwrap();
        let round = state.start_round().await.unwrap();

        // Setup full round
        let prompt = state
            .add_prompt(
                &round.id,
                Some("Test".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state.select_prompt(&round.id, &prompt.id).await.unwrap();

        let player = state.create_player().await;
        let player_sub = state
            .submit_answer(&round.id, Some(player.id.clone()), "Player".to_string())
            .await
            .unwrap();
        let ai_sub = state
            .submit_answer(&round.id, None, "AI".to_string())
            .await
            .unwrap();

        state
            .set_ai_submission(&round.id, ai_sub.id.clone())
            .await
            .unwrap();
        state
            .set_reveal_order(&round.id, vec![player_sub.id.clone(), ai_sub.id.clone()])
            .await
            .unwrap();

        // Add a vote
        let vote = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round.id.clone(),
            voter_id: "voter1".to_string(),
            ai_pick_submission_id: player_sub.id.clone(),
            funny_pick_submission_id: player_sub.id.clone(),
            ts: chrono::Utc::now().to_rfc3339(),
        };
        state.votes.write().await.insert(vote.id.clone(), vote);

        // Progress to RESULTS (first time)
        state.transition_phase(GamePhase::Writing).await.unwrap();
        state.transition_phase(GamePhase::Reveal).await.unwrap();
        state.transition_phase(GamePhase::Voting).await.unwrap();
        state.transition_phase(GamePhase::Results).await.unwrap();

        let (scores1, _) = state.get_leaderboards().await;
        assert_eq!(scores1.len(), 1);
        assert_eq!(scores1[0].total, 2); // 1 AI + 1 funny

        // Re-enter RESULTS (should not duplicate scores)
        state
            .transition_phase(GamePhase::Intermission)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Results).await.unwrap();

        let (scores2, _) = state.get_leaderboards().await;
        assert_eq!(scores2.len(), 1);
        assert_eq!(scores2[0].total, 2); // Still 2, not 4!
    }

    #[tokio::test]
    async fn test_exact_duplicate_detection() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // First submission succeeds
        let result = state
            .submit_answer(&round.id, None, "Test answer".to_string())
            .await;
        assert!(result.is_ok());

        // Exact duplicate fails
        let result = state
            .submit_answer(&round.id, None, "Test answer".to_string())
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "DUPLICATE_EXACT");
    }

    #[tokio::test]
    async fn test_duplicate_detection_case_insensitive() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // First submission succeeds
        state
            .submit_answer(&round.id, None, "Test Answer".to_string())
            .await
            .unwrap();

        // Same text different case fails
        let result = state
            .submit_answer(&round.id, None, "test answer".to_string())
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "DUPLICATE_EXACT");

        // Different case with whitespace also fails
        let result = state
            .submit_answer(&round.id, None, "  TEST ANSWER  ".to_string())
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "DUPLICATE_EXACT");
    }

    #[tokio::test]
    async fn test_mark_submission_duplicate() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create a player and their submission
        let player = state.create_player().await;
        let sub = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Player answer".to_string(),
            )
            .await
            .unwrap();

        // Verify submission exists
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 1);

        // Mark as duplicate
        let result = state.mark_submission_duplicate(&sub.id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(player.id));

        // Verify submission is removed
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 0);
    }

    #[tokio::test]
    async fn test_mark_ai_submission_duplicate() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create AI submission
        let sub = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();

        // Mark as duplicate - returns None for AI submissions
        let result = state.mark_submission_duplicate(&sub.id).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Verify submission is removed
        let submissions = state.get_submissions(&round.id).await;
        assert_eq!(submissions.len(), 0);
    }

    #[tokio::test]
    async fn test_mark_nonexistent_duplicate() {
        let state = AppState::new();
        state.create_game().await;

        let result = state.mark_submission_duplicate("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // Shadowban tests

    #[tokio::test]
    async fn test_shadowban_audience() {
        let state = AppState::new();

        // Initially not shadowbanned
        assert!(!state.is_shadowbanned("voter1").await);

        // Shadowban the voter
        state.shadowban_audience("voter1".to_string()).await;

        // Now should be shadowbanned
        assert!(state.is_shadowbanned("voter1").await);

        // Other voters unaffected
        assert!(!state.is_shadowbanned("voter2").await);
    }

    #[tokio::test]
    async fn test_shadowban_filters_prompts() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add prompts from different audience members
        state
            .add_prompt(
                &round.id,
                Some("Prompt from voter1".to_string()),
                None,
                PromptSource::Audience,
                Some("voter1".to_string()),
            )
            .await
            .unwrap();
        state
            .add_prompt(
                &round.id,
                Some("Prompt from voter2".to_string()),
                None,
                PromptSource::Audience,
                Some("voter2".to_string()),
            )
            .await
            .unwrap();
        state
            .add_prompt(
                &round.id,
                Some("Host prompt".to_string()),
                None,
                PromptSource::Host,
                None,
            )
            .await
            .unwrap();

        // Before shadowban: all 3 prompts should be visible
        let rounds = state.rounds.read().await;
        let round_data = rounds.get(&round.id).unwrap();
        assert_eq!(round_data.prompt_candidates.len(), 3);
        drop(rounds);

        // Shadowban voter1
        state.shadowban_audience("voter1".to_string()).await;

        // The prompts are still stored, but the broadcast filters them
        // Let's verify the shadowban set contains voter1
        assert!(state.is_shadowbanned("voter1").await);

        // Get shadowbanned list
        let shadowbanned = state.get_shadowbanned_audience().await;
        assert_eq!(shadowbanned.len(), 1);
        assert!(shadowbanned.contains(&"voter1".to_string()));
    }

    #[tokio::test]
    async fn test_prompt_submitter_id_tracked() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add a prompt with submitter_id
        let prompt = state
            .add_prompt(
                &round.id,
                Some("Test prompt".to_string()),
                None,
                PromptSource::Audience,
                Some("voter123".to_string()),
            )
            .await
            .unwrap();

        // Verify submitter_id was stored
        assert_eq!(prompt.submitter_id, Some("voter123".to_string()));

        // Verify it's in the round's prompt_candidates
        let rounds = state.rounds.read().await;
        let round_data = rounds.get(&round.id).unwrap();
        let stored_prompt = round_data
            .prompt_candidates
            .iter()
            .find(|p| p.id == prompt.id)
            .unwrap();
        assert_eq!(stored_prompt.submitter_id, Some("voter123".to_string()));
    }

    // Remove player tests

    #[tokio::test]
    async fn test_remove_player_basic() {
        let state = AppState::new();
        state.create_game().await;

        // Create a player
        let player = state.create_player().await;
        let player_id = player.id.clone();

        // Verify player exists
        assert!(state.get_player_by_token(&player.token).await.is_some());
        assert_eq!(state.players.read().await.len(), 1);

        // Remove the player
        let result = state.remove_player(&player_id).await;
        assert!(result.is_ok());

        // Verify player is gone
        assert!(state.get_player_by_token(&player.token).await.is_none());
        assert_eq!(state.players.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_player_not_found() {
        let state = AppState::new();
        state.create_game().await;

        let result = state.remove_player(&"nonexistent".to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_remove_player_removes_submission() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create player and submit answer
        let player = state.create_player().await;
        let player_id = player.id.clone();
        state
            .submit_answer(
                &round.id,
                Some(player_id.clone()),
                "Test answer".to_string(),
            )
            .await
            .unwrap();

        // Verify submission exists
        assert_eq!(state.get_submissions(&round.id).await.len(), 1);

        // Remove player
        state.remove_player(&player_id).await.unwrap();

        // Verify submission is removed
        assert_eq!(state.get_submissions(&round.id).await.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_player_updates_reveal_order() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create two players with submissions
        let player1 = state.create_player().await;
        let player2 = state.create_player().await;

        let sub1 = state
            .submit_answer(&round.id, Some(player1.id.clone()), "Answer 1".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, Some(player2.id.clone()), "Answer 2".to_string())
            .await
            .unwrap();

        // Set reveal order
        state
            .set_reveal_order(&round.id, vec![sub1.id.clone(), sub2.id.clone()])
            .await
            .unwrap();

        // Verify reveal order has both
        let round_data = state.get_current_round().await.unwrap();
        assert_eq!(round_data.reveal_order.len(), 2);

        // Remove player1
        state.remove_player(&player1.id).await.unwrap();

        // Verify reveal order only has player2's submission
        let round_data = state.get_current_round().await.unwrap();
        assert_eq!(round_data.reveal_order.len(), 1);
        assert_eq!(round_data.reveal_order[0], sub2.id);
    }

    #[tokio::test]
    async fn test_remove_player_resets_affected_votes() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create player with submission
        let player = state.create_player().await;
        let sub = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Player answer".to_string(),
            )
            .await
            .unwrap();

        // Create another submission (AI) to vote for funny
        let ai_sub = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();

        // Add a vote that references the player's submission
        state
            .submit_vote(
                "voter1".to_string(),
                sub.id.clone(), // AI pick points to player's submission
                ai_sub.id.clone(),
                "msg1".to_string(),
            )
            .await;

        // Another vote that doesn't reference the player's submission
        state
            .submit_vote(
                "voter2".to_string(),
                ai_sub.id.clone(),
                ai_sub.id.clone(),
                "msg2".to_string(),
            )
            .await;

        // Verify we have 2 votes
        assert_eq!(state.votes.read().await.len(), 2);

        // Remove player
        state.remove_player(&player.id).await.unwrap();

        // voter1's vote should be removed (referenced player's submission)
        // voter2's vote should remain (only referenced AI submission)
        assert_eq!(state.votes.read().await.len(), 1);

        // voter1 should be able to vote again (msg_id cleared)
        let processed = state.processed_vote_msg_ids.read().await;
        assert!(!processed.contains_key("voter1"));
        assert!(processed.contains_key("voter2"));
    }

    #[tokio::test]
    async fn test_remove_player_adjusts_reveal_index() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Create two players with submissions
        let player1 = state.create_player().await;
        let player2 = state.create_player().await;

        let sub1 = state
            .submit_answer(&round.id, Some(player1.id.clone()), "Answer 1".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, Some(player2.id.clone()), "Answer 2".to_string())
            .await
            .unwrap();

        // Set reveal order and advance reveal index to end
        state
            .set_reveal_order(&round.id, vec![sub1.id.clone(), sub2.id.clone()])
            .await
            .unwrap();

        // Advance reveal to the second submission (index 1)
        state.reveal_next(&round.id).await.unwrap();

        // Verify reveal_index is 1
        let round_data = state.get_current_round().await.unwrap();
        assert_eq!(round_data.reveal_index, 1);

        // Remove player2 (whose submission is at the current reveal index)
        state.remove_player(&player2.id).await.unwrap();

        // reveal_index should be adjusted to remain in bounds
        let round_data = state.get_current_round().await.unwrap();
        assert_eq!(round_data.reveal_order.len(), 1);
        assert_eq!(round_data.reveal_index, 0); // Adjusted to last valid index
    }

    #[tokio::test]
    async fn test_remove_player_clears_status() {
        let state = AppState::new();
        state.create_game().await;

        // Create player and set status
        let player = state.create_player().await;
        state
            .set_player_status(&player.id, PlayerSubmissionStatus::Submitted)
            .await;

        // Verify status is set
        assert_eq!(
            state.get_player_status(&player.id).await,
            PlayerSubmissionStatus::Submitted
        );

        // Remove player
        state.remove_player(&player.id).await.unwrap();

        // Status should be cleared (returns default NotSubmitted for unknown player)
        assert_eq!(
            state.get_player_status(&player.id).await,
            PlayerSubmissionStatus::NotSubmitted
        );
    }

    #[tokio::test]
    async fn test_remove_player_no_round() {
        let state = AppState::new();
        state.create_game().await;
        // Don't start a round

        // Create a player
        let player = state.create_player().await;

        // Remove should still work (no submissions to clean up)
        let result = state.remove_player(&player.id).await;
        assert!(result.is_ok());

        // Player should be gone
        assert_eq!(state.players.read().await.len(), 0);
    }
}
