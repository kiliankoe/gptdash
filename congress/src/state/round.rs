use super::AppState;
use crate::llm::GenerateRequest;
use crate::protocol::ServerMessage;
use crate::types::*;
use std::collections::HashSet;

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
            selected_prompt: None,
            submission_deadline: None,
            reveal_order: Vec::new(),
            reveal_index: 0,
            ai_submission_id: None,
            scored_at: None,
            manual_ai_winner: None,
            manual_funny_winner: None,
            results_step: 0,
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
        drop(game);

        // Reset per-round player status (submitted/checking) for the new round.
        self.clear_player_statuses().await;
        let players = self.get_all_player_status().await;
        self.broadcast_to_host(ServerMessage::HostPlayerStatus { players });

        // Broadcast round change so all clients can reset per-round UI state,
        // even when the round is started implicitly during phase transitions.
        self.broadcast_to_all(ServerMessage::RoundStarted {
            round: round.clone(),
        });

        // Clear per-round views on clients by broadcasting an empty submissions list
        // for the new round (host + beamer + audience).
        self.broadcast_submissions(&round.id).await;

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

    /// Select a prompt from the pool for the round and transition to Collecting
    /// This removes the prompt from the pool (it's been "used")
    /// `model` specifies which AI model to use (e.g., "openai:gpt-5"). If None, uses all providers.
    pub async fn select_prompt(
        &self,
        round_id: &str,
        prompt_id: &str,
        model: Option<String>,
    ) -> Result<Prompt, String> {
        // Get and remove the prompt from the pool
        let prompt = self
            .remove_prompt_from_pool(prompt_id)
            .await
            .ok_or_else(|| "Prompt not found in pool".to_string())?;

        // Update the round
        {
            let mut rounds = self.rounds.write().await;
            if let Some(round) = rounds.get_mut(round_id) {
                // Validate current state
                if round.state != RoundState::Setup {
                    // Put prompt back in pool since we couldn't use it
                    self.prompt_pool
                        .write()
                        .await
                        .insert(prompt.id.clone(), prompt);
                    return Err(format!(
                        "Can only select prompt in Setup state, currently in {:?}",
                        round.state
                    ));
                }

                round.selected_prompt = Some(prompt.clone());
                round.state = RoundState::Collecting;
            } else {
                // Put prompt back in pool since round doesn't exist
                self.prompt_pool
                    .write()
                    .await
                    .insert(prompt.id.clone(), prompt);
                return Err("Round not found".to_string());
            }
        }

        // Broadcast updated prompt pool to host and selected prompt to all clients
        // (needed for prompts selected implicitly during phase transitions).
        self.broadcast_prompts_to_host().await;
        self.broadcast_to_all(ServerMessage::PromptSelected {
            prompt: prompt.clone(),
        });

        // Kick off LLM generation in the background (don't block)
        // Only generate if there's text or an image to work with
        if prompt.text.is_some() || prompt.image_url.is_some() {
            let state = self.clone();
            let round_id = round_id.to_string();
            let prompt_clone = prompt.clone();
            let model_clone = model.clone();
            tokio::spawn(async move {
                if let Err(e) = state
                    .generate_ai_submissions(&round_id, &prompt_clone, model_clone.as_deref())
                    .await
                {
                    tracing::error!("Failed to generate AI submissions: {}", e);
                    // Notify host about the failure
                    let _ = state
                        .host_broadcast
                        .send(crate::protocol::ServerMessage::Error {
                            code: "LLM_GENERATION_FAILED".to_string(),
                            msg: format!("AI generation failed: {}", e),
                        });
                }
            });
        }

        Ok(prompt)
    }

    /// Generate AI submissions from LLM providers
    /// If `model` is Some, generates from that specific model only (format: "provider:model")
    /// If `model` is None, generates from all available providers (existing behavior)
    pub async fn generate_ai_submissions(
        &self,
        round_id: &str,
        prompt: &Prompt,
        model: Option<&str>,
    ) -> Result<(), String> {
        let llm = match &self.llm {
            Some(manager) => manager,
            None => {
                tracing::warn!("No LLM providers available, skipping AI generation");
                return Ok(());
            }
        };

        let prompt_text = prompt.text.as_deref().unwrap_or("");
        let is_multimodal = prompt.image_url.is_some();

        tracing::info!(
            "Generating AI submissions for prompt: {} (multimodal: {})",
            prompt_text,
            is_multimodal
        );

        // Get game config for max_answer_chars
        let max_chars = {
            let game = self.game.read().await;
            game.as_ref()
                .map(|g| g.config.max_answer_chars)
                .unwrap_or(500)
        };

        let request = GenerateRequest {
            prompt: prompt_text.to_string(),
            image_url: prompt.image_url.clone(),
            max_tokens: Some(self.llm_config.default_max_tokens),
            timeout: self.llm_config.default_timeout,
            model_override: None, // Model override is handled by generate_from_model
        };

        // Generate either from specific model or all providers
        let responses: Vec<(String, crate::llm::GenerateResponse)> = if let Some(model_id) = model {
            // Generate from specific model only
            tracing::info!("Generating AI submission from model: {}", model_id);
            match llm.generate_from_model(model_id, request).await {
                Ok(response) => vec![response],
                Err(e) => {
                    tracing::error!("Model {} failed: {}", model_id, e);
                    return Err(format!("Model {} failed: {}", model_id, e));
                }
            }
        } else {
            // Generate from all providers concurrently
            llm.generate_from_all(request).await
        };

        if responses.is_empty() {
            tracing::error!("No LLM providers generated responses");
            return Err("All LLM providers failed".to_string());
        }

        // If an AI answer was previously selected, try to keep the selection on regeneration
        // by re-selecting the new answer from the same provider.
        let selected_ai_provider: Option<String> = {
            let selected_id = {
                let rounds = self.rounds.read().await;
                rounds
                    .get(round_id)
                    .and_then(|r| r.ai_submission_id.clone())
            };
            if let Some(id) = selected_id {
                let submissions = self.submissions.read().await;
                submissions
                    .get(&id)
                    .and_then(|s| s.author_ref.clone())
                    .and_then(|r| r.split(':').next().map(|p| p.to_string()))
            } else {
                None
            }
        };

        // Track generated submission IDs for auto-selection
        let mut generated_submission_ids = Vec::new();

        // Store each AI submission with provider metadata
        for (provider_name, response) in responses {
            // Remove older submissions from this provider to avoid duplicates on regeneration.
            let provider_prefix = format!("{}:", provider_name);
            let removed_ids: HashSet<SubmissionId> = {
                let submissions = self.submissions.read().await;
                submissions
                    .values()
                    .filter(|s| {
                        s.round_id == round_id
                            && s.author_kind == AuthorKind::Ai
                            && s.author_ref
                                .as_deref()
                                .is_some_and(|r| r.starts_with(&provider_prefix))
                    })
                    .map(|s| s.id.clone())
                    .collect()
            };

            if !removed_ids.is_empty() {
                let mut submissions = self.submissions.write().await;
                let mut by_round = self.submissions_by_round.write().await;
                for id in &removed_ids {
                    submissions.remove(id);
                    if let Some(round_subs) = by_round.get_mut(round_id) {
                        round_subs.remove(id);
                    }
                }
                drop(by_round);
                drop(submissions);
                self.cleanup_round_after_submission_removals(round_id, &removed_ids)
                    .await;
            }

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

            // Insert into primary store and secondary index
            {
                let mut submissions = self.submissions.write().await;
                let mut by_round = self.submissions_by_round.write().await;
                submissions.insert(submission_id.clone(), submission.clone());
                by_round
                    .entry(round_id.to_string())
                    .or_default()
                    .insert(submission_id.clone());
            }

            generated_submission_ids.push(submission_id.clone());

            // If the host had selected an AI answer previously, keep the selection on the same
            // provider when regenerating (as long as that provider returned a new answer).
            if selected_ai_provider.as_deref() == Some(provider_name.as_str()) {
                let mut rounds = self.rounds.write().await;
                if let Some(round) = rounds.get_mut(round_id) {
                    round.ai_submission_id = Some(submission_id.clone());
                }
            }

            tracing::info!(
                "Generated AI submission from {}: {} chars in {}ms",
                provider_name,
                response.text.len(),
                response.metadata.latency_ms
            );
        }

        // Auto-select the AI submission if only one was generated
        // (host can still override with HostSetAiSubmission if multiple providers exist)
        if generated_submission_ids.len() == 1 {
            let submission_id = &generated_submission_ids[0];
            let mut rounds = self.rounds.write().await;
            if let Some(round) = rounds.get_mut(round_id) {
                round.ai_submission_id = Some(submission_id.clone());
                tracing::info!(
                    "Auto-selected AI submission {} (single provider)",
                    submission_id
                );
            }
        } else {
            tracing::info!(
                "Multiple AI submissions generated ({}), host must select one",
                generated_submission_ids.len()
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
    /// Only AI-authored submissions can be selected (not player submissions)
    pub async fn set_ai_submission(
        &self,
        round_id: &str,
        submission_id: SubmissionId,
    ) -> Result<(), String> {
        // Validate submission exists, belongs to this round, and is AI-authored
        let submissions = self.submissions.read().await;
        match submissions.get(&submission_id) {
            Some(sub) if sub.round_id == round_id => {
                // Must be an AI-authored submission
                if sub.author_kind != AuthorKind::Ai {
                    return Err(
                        "Only AI-generated submissions can be selected as the AI answer"
                            .to_string(),
                    );
                }
            }
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

    /// Advance to next submission in reveal carousel
    pub async fn reveal_next(&self, round_id: &str) -> Result<usize, String> {
        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            if round.reveal_order.is_empty() {
                return Err("No reveal order set".to_string());
            }

            let max_index = round.reveal_order.len() - 1;
            if round.reveal_index < max_index {
                round.reveal_index += 1;
            }
            Ok(round.reveal_index)
        } else {
            Err("Round not found".to_string())
        }
    }

    /// Go back to previous submission in reveal carousel
    pub async fn reveal_prev(&self, round_id: &str) -> Result<usize, String> {
        let mut rounds = self.rounds.write().await;
        if let Some(round) = rounds.get_mut(round_id) {
            if round.reveal_order.is_empty() {
                return Err("No reveal order set".to_string());
            }

            if round.reveal_index > 0 {
                round.reveal_index -= 1;
            }
            Ok(round.reveal_index)
        } else {
            Err("Round not found".to_string())
        }
    }

    /// Get current submission in reveal carousel
    pub async fn get_current_reveal_submission(
        &self,
        round_id: &str,
    ) -> Option<crate::types::Submission> {
        let submission_id = {
            let rounds = self.rounds.read().await;
            if let Some(round) = rounds.get(round_id) {
                if round.reveal_index < round.reveal_order.len() {
                    Some(round.reveal_order[round.reveal_index].clone())
                } else {
                    None
                }
            } else {
                None
            }
        }?;

        let submissions = self.submissions.read().await;
        submissions.get(&submission_id).cloned()
    }
}
