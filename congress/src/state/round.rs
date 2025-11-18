use super::AppState;
use crate::types::*;

impl AppState {
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

        self.rounds
            .write()
            .await
            .insert(round.id.clone(), round.clone());
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
    pub async fn add_prompt(
        &self,
        round_id: &str,
        text: String,
        source: PromptSource,
    ) -> Result<Prompt, String> {
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
            if let Some(prompt) = round
                .prompt_candidates
                .iter()
                .find(|p| p.id == prompt_id)
                .cloned()
            {
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

    /// Set reveal order
    pub async fn set_reveal_order(
        &self,
        round_id: &str,
        order: Vec<SubmissionId>,
    ) -> Result<(), String> {
        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            round.reveal_order = order;
            Ok(())
        } else {
            Err("Round not found".to_string())
        }
    }
}
