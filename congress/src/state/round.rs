use super::AppState;
use crate::llm::GenerateRequest;
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
    #[cfg_attr(not(test), allow(dead_code))]
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
    #[cfg_attr(not(test), allow(dead_code))]
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
    #[cfg_attr(not(test), allow(dead_code))]
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
        let prompt_text = {
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
                    let text = prompt.text.clone();
                    round.selected_prompt = Some(prompt);
                    round.state = RoundState::Collecting;
                    text
                } else {
                    return Err("Prompt not found".to_string());
                }
            } else {
                return Err("Round not found".to_string());
            }
        };

        // Kick off LLM generation in the background (don't block)
        if let Some(text) = prompt_text {
            let state = self.clone();
            let round_id = round_id.to_string();
            tokio::spawn(async move {
                if let Err(e) = state.generate_ai_submissions(&round_id, &text).await {
                    tracing::error!("Failed to generate AI submissions: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Generate AI submissions from all available LLM providers
    pub async fn generate_ai_submissions(
        &self,
        round_id: &str,
        prompt_text: &str,
    ) -> Result<(), String> {
        let llm = match &self.llm {
            Some(manager) => manager,
            None => {
                tracing::warn!("No LLM providers available, skipping AI generation");
                return Ok(());
            }
        };

        tracing::info!("Generating AI submissions for prompt: {}", prompt_text);

        // Get game config for max_answer_chars
        let max_chars = {
            let game = self.game.read().await;
            game.as_ref()
                .map(|g| g.config.max_answer_chars)
                .unwrap_or(500)
        };

        let request = GenerateRequest {
            prompt: prompt_text.to_string(),
            image_url: None,
            max_tokens: Some(self.llm_config.default_max_tokens),
            timeout: self.llm_config.default_timeout,
        };

        // Generate from all providers concurrently
        let responses = llm.generate_from_all(request).await;

        if responses.is_empty() {
            tracing::error!("No LLM providers generated responses");
            return Err("All LLM providers failed".to_string());
        }

        // Store each AI submission with provider metadata
        for (provider_name, response) in responses {
            let submission_id = ulid::Ulid::new().to_string();

            // Truncate if needed
            let text = if response.text.len() > max_chars {
                response.text.chars().take(max_chars).collect()
            } else {
                response.text.clone()
            };

            let submission = Submission {
                id: submission_id.clone(),
                round_id: round_id.to_string(),
                author_kind: AuthorKind::Ai,
                author_ref: Some(format!("{}:{}", provider_name, response.metadata.model)),
                original_text: text.clone(),
                display_text: text,
                edited_by_host: Some(false),
                tts_asset_url: None,
            };

            self.submissions
                .write()
                .await
                .insert(submission_id.clone(), submission.clone());

            tracing::info!(
                "Generated AI submission from {}: {} chars in {}ms",
                provider_name,
                response.text.len(),
                response.metadata.latency_ms
            );
        }

        // Broadcast updated submissions list to all clients
        self.broadcast_submissions(round_id).await;

        Ok(())
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
