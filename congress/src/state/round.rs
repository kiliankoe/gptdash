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
            scored_at: None,
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
        // Validate game phase allows starting a new round
        let game = self.game.read().await;
        if let Some(ref g) = *game {
            use GamePhase::*;
            match g.phase {
                Lobby | PromptSelection | Results | Podium => {
                    // Valid phases for starting a round
                }
                _ => {
                    return Err(format!(
                        "Cannot start round in {:?} phase. Must be in Lobby, PromptSelection, Results, or Podium",
                        g.phase
                    ));
                }
            }

            // If there's a current round, ensure it's closed
            if let Some(ref current_round_id) = g.current_round_id {
                let rounds = self.rounds.read().await;
                if let Some(current_round) = rounds.get(current_round_id) {
                    if current_round.state != RoundState::Closed {
                        return Err(format!(
                            "Cannot start new round while current round is in {:?} state",
                            current_round.state
                        ));
                    }
                }
            }
        } else {
            return Err("No active game".to_string());
        }
        drop(game);

        let round = self.create_round().await?;

        let mut game = self.game.write().await;
        if let Some(ref mut g) = *game {
            g.current_round_id = Some(round.id.clone());
            g.round_no = round.number;
            g.version += 1;
        }

        Ok(round)
    }

    /// Check if a round state transition is valid
    fn is_valid_round_state_transition(from: &RoundState, to: &RoundState) -> bool {
        use RoundState::*;

        matches!(
            (from, to),
            (Setup, Collecting)
                | (Collecting, Revealing)
                | (Revealing, OpenForVotes)
                | (OpenForVotes, Scoring)
                | (Scoring, Closed)
        )
    }

    /// Validate preconditions for a round state transition
    fn validate_round_state_preconditions(round: &Round, to: &RoundState) -> Result<(), String> {
        match to {
            RoundState::Collecting => {
                if round.selected_prompt.is_none() {
                    return Err("Collecting state requires a selected prompt".to_string());
                }
            }
            RoundState::Revealing => {
                // Will be checked by caller - needs submission count
            }
            RoundState::OpenForVotes => {
                if round.reveal_order.is_empty() {
                    return Err("OpenForVotes state requires reveal order to be set".to_string());
                }
            }
            _ => {} // No preconditions for other states
        }
        Ok(())
    }

    /// Transition round state with validation
    pub async fn transition_round_state(
        &self,
        round_id: &str,
        new_state: RoundState,
    ) -> Result<(), String> {
        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            let current_state = &round.state;

            // Validate transition is allowed
            if !Self::is_valid_round_state_transition(current_state, &new_state) {
                return Err(format!(
                    "Invalid round state transition from {:?} to {:?}",
                    current_state, new_state
                ));
            }

            // Validate preconditions
            Self::validate_round_state_preconditions(round, &new_state)?;

            // Special check for Revealing: needs submissions
            if new_state == RoundState::Revealing {
                let submissions = self.submissions.read().await;
                let submission_count = submissions
                    .values()
                    .filter(|s| s.round_id == round_id)
                    .count();
                if submission_count == 0 {
                    return Err("Revealing state requires at least one submission".to_string());
                }
            }

            round.state = new_state;
            Ok(())
        } else {
            Err("Round not found".to_string())
        }
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

    /// Select a prompt for the round and transition to Collecting
    pub async fn select_prompt(&self, round_id: &str, prompt_id: &str) -> Result<(), String> {
        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            // Validate current state
            if round.state != RoundState::Setup {
                return Err(format!(
                    "Can only select prompt in Setup state, currently in {:?}",
                    round.state
                ));
            }

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
        if order.is_empty() {
            return Err("Reveal order cannot be empty".to_string());
        }

        // Validate all submission IDs belong to this round
        let submissions = self.submissions.read().await;
        for submission_id in &order {
            match submissions.get(submission_id) {
                Some(submission) => {
                    if submission.round_id != round_id {
                        return Err(format!(
                            "Submission {} does not belong to round {}",
                            submission_id, round_id
                        ));
                    }
                }
                None => {
                    return Err(format!("Submission {} not found", submission_id));
                }
            }
        }
        drop(submissions);

        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            round.reveal_order = order;
            Ok(())
        } else {
            Err("Round not found".to_string())
        }
    }

    /// Set which submission is the AI submission for scoring
    pub async fn set_ai_submission(
        &self,
        round_id: &str,
        submission_id: SubmissionId,
    ) -> Result<(), String> {
        // Validate submission exists and belongs to this round
        let submissions = self.submissions.read().await;
        match submissions.get(&submission_id) {
            Some(sub) if sub.round_id == round_id => {}
            Some(_) => {
                return Err(format!(
                    "Submission {} does not belong to round {}",
                    submission_id, round_id
                ));
            }
            None => {
                return Err(format!("Submission {} not found", submission_id));
            }
        }
        drop(submissions);

        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            round.ai_submission_id = Some(submission_id);
            Ok(())
        } else {
            Err("Round not found".to_string())
        }
    }
}
