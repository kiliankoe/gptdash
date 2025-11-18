use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub game: Arc<RwLock<Option<Game>>>,
    pub rounds: Arc<RwLock<HashMap<RoundId, Round>>>,
    pub submissions: Arc<RwLock<HashMap<SubmissionId, Submission>>>,
    pub votes: Arc<RwLock<HashMap<VoteId, Vote>>>,
    pub players: Arc<RwLock<HashMap<PlayerId, Player>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            game: Arc::new(RwLock::new(None)),
            rounds: Arc::new(RwLock::new(HashMap::new())),
            submissions: Arc::new(RwLock::new(HashMap::new())),
            votes: Arc::new(RwLock::new(HashMap::new())),
            players: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_game(&self) -> Game {
        let game = Game {
            id: ulid::Ulid::new().to_string(),
            version: 1,
            phase: GamePhase::Lobby,
            round_no: 0,
            config: GameConfig::default(),
            current_round_id: None,
        };

        *self.game.write().await = Some(game.clone());
        game
    }

    pub async fn get_game(&self) -> Option<Game> {
        self.game.read().await.clone()
    }

    pub async fn create_player(&self) -> Player {
        let player = Player {
            id: ulid::Ulid::new().to_string(),
            token: ulid::Ulid::new().to_string(),
            display_name: None,
        };

        self.players
            .write()
            .await
            .insert(player.id.clone(), player.clone());
        player
    }

    /// Register a player with display name
    pub async fn register_player(&self, token: &str, display_name: String) -> Result<Player, String> {
        let mut players = self.players.write().await;

        // Find player by token
        if let Some(player) = players.values_mut().find(|p| p.token == token) {
            player.display_name = Some(display_name.clone());
            Ok(player.clone())
        } else {
            Err("Invalid player token".to_string())
        }
    }

    /// Get player by token
    pub async fn get_player_by_token(&self, token: &str) -> Option<Player> {
        self.players.read().await.values().find(|p| p.token == token).cloned()
    }

    /// Transition game phase
    pub async fn transition_phase(&self, new_phase: GamePhase) -> Result<(), String> {
        let mut game = self.game.write().await;
        if let Some(ref mut g) = *game {
            g.phase = new_phase;
            g.version += 1;
            Ok(())
        } else {
            Err("No active game".to_string())
        }
    }

    /// Create a new round
    pub async fn create_round(&self) -> Result<Round, String> {
        let game = self.game.read().await;
        let game = game.as_ref().ok_or("No active game")?;

        let round = Round {
            id: ulid::Ulid::new().to_string(),
            game_id: game.id.clone(),
            number: game.round_no + 1,
            state: RoundState::Setup,
            prompt_candidates: Vec::new(),
            selected_prompt: None,
            submission_deadline: None,
            reveal_order: Vec::new(),
            ai_submission_id: None,
        };

        self.rounds.write().await.insert(round.id.clone(), round.clone());
        Ok(round)
    }

    /// Get current round
    pub async fn get_current_round(&self) -> Option<Round> {
        let game = self.game.read().await;
        if let Some(ref g) = *game {
            if let Some(ref round_id) = g.current_round_id {
                return self.rounds.read().await.get(round_id).cloned();
            }
        }
        None
    }

    /// Start a new round
    pub async fn start_round(&self) -> Result<Round, String> {
        let round = self.create_round().await?;

        let mut game = self.game.write().await;
        if let Some(ref mut g) = *game {
            g.current_round_id = Some(round.id.clone());
            g.round_no = round.number;
            g.version += 1;
        }

        Ok(round)
    }

    /// Add a prompt candidate
    pub async fn add_prompt(&self, round_id: &str, text: String, source: PromptSource) -> Result<Prompt, String> {
        let prompt = Prompt {
            id: ulid::Ulid::new().to_string(),
            text: Some(text),
            image_url: None,
            source,
        };

        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            round.prompt_candidates.push(prompt.clone());
            Ok(prompt)
        } else {
            Err("Round not found".to_string())
        }
    }

    /// Select a prompt for the round
    pub async fn select_prompt(&self, round_id: &str, prompt_id: &str) -> Result<(), String> {
        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            if let Some(prompt) = round.prompt_candidates.iter().find(|p| p.id == prompt_id).cloned() {
                round.selected_prompt = Some(prompt);
                round.state = RoundState::Collecting;
                Ok(())
            } else {
                Err("Prompt not found".to_string())
            }
        } else {
            Err("Round not found".to_string())
        }
    }

    /// Submit an answer
    pub async fn submit_answer(&self, round_id: &str, player_id: Option<String>, text: String) -> Result<Submission, String> {
        let submission = Submission {
            id: ulid::Ulid::new().to_string(),
            round_id: round_id.to_string(),
            author_kind: if player_id.is_some() { AuthorKind::Player } else { AuthorKind::Ai },
            author_ref: player_id,
            original_text: text.clone(),
            display_text: text,
            edited_by_host: Some(false),
            tts_asset_url: None,
        };

        self.submissions.write().await.insert(submission.id.clone(), submission.clone());
        Ok(submission)
    }

    /// Edit a submission (host only)
    pub async fn edit_submission(&self, submission_id: &str, new_text: String) -> Result<(), String> {
        let mut submissions = self.submissions.write().await;
        if let Some(submission) = submissions.get_mut(submission_id) {
            submission.display_text = new_text;
            submission.edited_by_host = Some(true);
            Ok(())
        } else {
            Err("Submission not found".to_string())
        }
    }

    /// Get submissions for a round
    pub async fn get_submissions(&self, round_id: &str) -> Vec<Submission> {
        self.submissions
            .read()
            .await
            .values()
            .filter(|s| s.round_id == round_id)
            .cloned()
            .collect()
    }

    /// Set reveal order
    pub async fn set_reveal_order(&self, round_id: &str, order: Vec<SubmissionId>) -> Result<(), String> {
        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            round.reveal_order = order;
            Ok(())
        } else {
            Err("Round not found".to_string())
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
