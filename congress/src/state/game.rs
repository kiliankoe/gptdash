use super::AppState;
use crate::types::*;

impl AppState {
    /// Initialize a new game
    pub async fn create_game(&self) -> Game {
        let game = Game {
            id: ulid::Ulid::new().to_string(),
            version: 1,
            phase: GamePhase::Lobby,
            round_no: 0,
            config: GameConfig::default(),
            current_round_id: None,
            phase_deadline: None,
            panic_mode: false,
        };

        *self.game.write().await = Some(game.clone());
        game
    }

    /// Get current game
    pub async fn get_game(&self) -> Option<Game> {
        self.game.read().await.clone()
    }

    /// Check if a phase transition is valid
    fn is_valid_phase_transition(from: &GamePhase, to: &GamePhase) -> bool {
        Self::get_valid_transitions(from).contains(to)
    }

    /// Get all valid transitions from a given phase
    pub fn get_valid_transitions(from: &GamePhase) -> Vec<GamePhase> {
        use GamePhase::*;

        match from {
            // Lobby can go directly to Writing if a prompt is selected (skips PromptSelection)
            Lobby => vec![PromptSelection, Writing, Intermission, Ended],
            PromptSelection => vec![Writing, Intermission, Ended],
            Writing => vec![Reveal, Intermission, Ended],
            Reveal => vec![Voting, Intermission, Ended],
            Voting => vec![Results, Intermission, Ended],
            // Results/Podium can go back to Lobby for new game or PromptSelection for next round
            Results => vec![Podium, Lobby, PromptSelection, Intermission, Ended],
            Podium => vec![Lobby, PromptSelection, Intermission, Ended],
            Intermission => vec![
                Lobby,
                PromptSelection,
                Writing,
                Reveal,
                Voting,
                Results,
                Podium,
                Ended,
            ],
            Ended => vec![Intermission],
        }
    }

    /// Validate preconditions for a phase transition
    async fn validate_phase_preconditions(
        &self,
        game: &Game,
        to: &GamePhase,
    ) -> Result<(), String> {
        match to {
            GamePhase::PromptSelection => {
                // Requires at least 1 queued prompt
                let queued = self.queued_prompts.read().await;
                if queued.is_empty() {
                    return Err("Prompt selection requires at least 1 queued prompt".to_string());
                }
            }
            GamePhase::Writing => {
                // Requires a current round with selected prompt
                // (Exception: when coming from PromptSelection, we select the winning prompt first)
                if game.phase != GamePhase::PromptSelection {
                    if let Some(round_id) = &game.current_round_id {
                        let rounds = self.rounds.read().await;
                        if let Some(round) = rounds.get(round_id) {
                            if round.selected_prompt.is_none() {
                                return Err("Writing phase requires a selected prompt".to_string());
                            }
                        } else {
                            return Err("Current round not found".to_string());
                        }
                    } else {
                        return Err("Writing phase requires an active round".to_string());
                    }
                }
                // If coming from PromptSelection, we'll select the winning prompt in the transition
            }
            GamePhase::Reveal => {
                // Requires submissions in current round
                if let Some(round_id) = &game.current_round_id {
                    let submissions = self.submissions.read().await;
                    let round_submissions: Vec<_> = submissions
                        .values()
                        .filter(|s| s.round_id == *round_id)
                        .collect();
                    if round_submissions.is_empty() {
                        return Err("Reveal phase requires at least one submission".to_string());
                    }
                } else {
                    return Err("Reveal phase requires an active round".to_string());
                }
            }
            GamePhase::Voting => {
                // Requires reveal order set
                if let Some(round_id) = &game.current_round_id {
                    let rounds = self.rounds.read().await;
                    if let Some(round) = rounds.get(round_id) {
                        if round.reveal_order.is_empty() {
                            return Err("Voting phase requires reveal order to be set".to_string());
                        }
                    } else {
                        return Err("Current round not found".to_string());
                    }
                } else {
                    return Err("Voting phase requires an active round".to_string());
                }
            }
            GamePhase::Results => {
                // Requires AI submission to be designated for scoring
                if let Some(round_id) = &game.current_round_id {
                    let rounds = self.rounds.read().await;
                    if let Some(round) = rounds.get(round_id) {
                        if round.ai_submission_id.is_none() {
                            return Err("Results phase requires AI submission to be set (use HostSetAiSubmission)".to_string());
                        }
                    } else {
                        return Err("Current round not found".to_string());
                    }
                } else {
                    return Err("Results phase requires an active round".to_string());
                }
            }
            _ => {} // No preconditions for other phases
        }
        Ok(())
    }

    /// Transition game phase with validation
    pub async fn transition_phase(&self, new_phase: GamePhase) -> Result<(), String> {
        let mut game = self.game.write().await;
        if let Some(ref mut g) = *game {
            let current_phase = &g.phase;

            // Validate transition is allowed
            if !Self::is_valid_phase_transition(current_phase, &new_phase) {
                return Err(format!(
                    "Invalid phase transition from {:?} to {:?}",
                    current_phase, new_phase
                ));
            }

            // Release lock temporarily to check preconditions
            let game_clone = g.clone();
            drop(game);

            // Validate preconditions
            self.validate_phase_preconditions(&game_clone, &new_phase)
                .await?;

            // Special case: PromptSelection with 1 prompt should skip directly to Writing
            // Handle round creation BEFORE changing phase since start_round validates current phase
            let effective_phase = if new_phase == GamePhase::PromptSelection {
                let queued = self.queued_prompts.read().await;
                if queued.len() == 1 {
                    // Single prompt: set up round and go directly to Writing
                    let prompt = queued[0].clone();
                    drop(queued);

                    // Put prompt back in pool so select_prompt can find it
                    self.prompt_pool.write().await.push(prompt.clone());
                    self.queued_prompts.write().await.clear();
                    self.prompt_votes.write().await.clear();
                    self.broadcast_queued_prompts_to_host().await;

                    // Create a new round BEFORE phase changes (start_round checks current phase)
                    match self.start_round().await {
                        Ok(round) => {
                            // Select the prompt (removes from pool, starts LLM)
                            // Use None for model to generate from all providers (auto-transition)
                            if let Err(e) = self.select_prompt(&round.id, &prompt.id, None).await {
                                tracing::error!("Failed to select prompt: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to start round: {}", e);
                        }
                    }

                    // Skip to Writing phase instead of PromptSelection
                    GamePhase::Writing
                } else {
                    drop(queued);
                    new_phase.clone()
                }
            } else if new_phase == GamePhase::Writing
                && game_clone.phase == GamePhase::PromptSelection
            {
                // Transitioning from PromptSelection to Writing: select winning prompt
                // IMPORTANT: Create round BEFORE phase changes (start_round checks current phase)
                match self.select_winning_prompt().await {
                    Ok(prompt) => {
                        // Put winning prompt back in pool so select_prompt can find it
                        // (select_winning_prompt removes it from queue but doesn't add to pool)
                        self.prompt_pool.write().await.push(prompt.clone());

                        // Clear queue and votes (may already be cleared by select_winning_prompt)
                        self.queued_prompts.write().await.clear();
                        self.prompt_votes.write().await.clear();
                        self.broadcast_queued_prompts_to_host().await;

                        // Create a new round BEFORE phase changes
                        match self.start_round().await {
                            Ok(round) => {
                                // Select the prompt (removes from pool, starts LLM)
                                // Use None for model to generate from all providers (auto-transition)
                                if let Err(e) =
                                    self.select_prompt(&round.id, &prompt.id, None).await
                                {
                                    tracing::error!("Failed to select winning prompt: {}", e);
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to start round: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "No winning prompt found during PromptSelection->Writing: {}",
                            e
                        );
                    }
                }
                new_phase.clone()
            } else {
                new_phase.clone()
            };

            // Re-acquire lock and apply transition
            let mut game = self.game.write().await;
            if let Some(ref mut g) = *game {
                g.phase = effective_phase.clone();
                g.version += 1;

                // Set deadline for timed phases
                g.phase_deadline = match effective_phase {
                    GamePhase::Writing => {
                        let seconds = g.config.writing_seconds as i64;
                        Some((chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339())
                    }
                    GamePhase::Voting => {
                        let seconds = g.config.voting_seconds as i64;
                        Some((chrono::Utc::now() + chrono::Duration::seconds(seconds)).to_rfc3339())
                    }
                    _ => None, // Clear deadline for other phases
                };

                let round_id = g.current_round_id.clone();
                drop(game);

                // Keep RoundState in sync with the game phase so subsequent rounds can start
                // cleanly (start_round requires the previous round to be Closed).
                if let Some(rid) = &round_id {
                    let mut rounds = self.rounds.write().await;
                    if let Some(round) = rounds.get_mut(rid) {
                        round.state = match effective_phase {
                            GamePhase::Writing => RoundState::Collecting,
                            GamePhase::Reveal => RoundState::Revealing,
                            GamePhase::Voting => RoundState::OpenForVotes,
                            // Once we reach results, the round is effectively finished and
                            // must be startable from Results/Podium.
                            GamePhase::Results
                            | GamePhase::Podium
                            | GamePhase::PromptSelection
                            | GamePhase::Lobby => RoundState::Closed,
                            _ => round.state.clone(),
                        };
                    }
                }

                // Handle phase-specific actions
                if effective_phase == GamePhase::PromptSelection {
                    // Multiple prompts: broadcast candidates for voting
                    let queued = self.queued_prompts.read().await;
                    let candidates: Vec<_> = queued.clone();
                    drop(queued);
                    self.broadcast_to_all(crate::protocol::ServerMessage::PromptCandidates {
                        prompts: candidates,
                    });
                } else if effective_phase == GamePhase::Reveal {
                    // Auto-populate reveal_order if not set, then reset reveal to first submission
                    if let Some(rid) = &round_id {
                        // Get submissions for auto-populating reveal_order if needed
                        let submissions = self.get_submissions(rid).await;

                        let mut rounds = self.rounds.write().await;
                        if let Some(round) = rounds.get_mut(rid) {
                            // Auto-populate reveal_order if empty
                            // Filter to include: all player submissions + only the selected AI submission
                            if round.reveal_order.is_empty() && !submissions.is_empty() {
                                let selected_ai_id = round.ai_submission_id.clone();
                                round.reveal_order = submissions
                                    .iter()
                                    .filter(|s| {
                                        // Include player submissions
                                        if s.author_kind == AuthorKind::Player {
                                            return true;
                                        }
                                        // For AI submissions, only include the selected one
                                        if s.author_kind == AuthorKind::Ai {
                                            return selected_ai_id.as_ref() == Some(&s.id);
                                        }
                                        false
                                    })
                                    .map(|s| s.id.clone())
                                    .collect();
                                tracing::info!(
                                    "Auto-populated reveal_order with {} submissions (filtered from {} total)",
                                    round.reveal_order.len(),
                                    submissions.len()
                                );
                            }
                            round.reveal_index = 0; // Reset to first submission
                        }
                        drop(rounds);

                        // Broadcast first submission
                        if let Some(submission) = self.get_current_reveal_submission(rid).await {
                            self.broadcast_to_all(crate::protocol::ServerMessage::RevealUpdate {
                                reveal_index: 0,
                                submission: Some(crate::protocol::SubmissionInfo::from(
                                    &submission,
                                )),
                            });
                        }
                    }
                } else if effective_phase == GamePhase::Voting {
                    // Record voting phase start time for server-side timing validation
                    self.set_voting_phase_started().await;

                    // Generate and broadcast vote challenge for anti-automation
                    let nonce = self.generate_vote_challenge().await;
                    if let Some(rid) = &round_id {
                        self.broadcast_to_all(crate::protocol::ServerMessage::VoteChallenge {
                            nonce,
                            round_id: rid.clone(),
                        });
                    }

                    // Broadcast submissions to all clients (audience needs them for voting)
                    // This is necessary because broadcast_submissions filters by phase
                    // and audience doesn't receive submissions during WRITING/REVEAL
                    if let Some(rid) = &round_id {
                        self.broadcast_submissions(rid).await;
                    }
                } else if effective_phase == GamePhase::Results {
                    // Compute scores when entering RESULTS phase (idempotent)
                    if let Some(rid) = round_id {
                        // Check if already scored
                        let rounds = self.rounds.read().await;
                        let already_scored = rounds
                            .get(&rid)
                            .map(|r| r.scored_at.is_some())
                            .unwrap_or(false);
                        drop(rounds);

                        if !already_scored {
                            match self.compute_scores(&rid).await {
                                Ok(_) => {
                                    // Mark round as scored
                                    let mut rounds = self.rounds.write().await;
                                    if let Some(round) = rounds.get_mut(&rid) {
                                        round.scored_at = Some(chrono::Utc::now().to_rfc3339());
                                    }
                                    drop(rounds);

                                    // Broadcast scores to all clients
                                    let (all_players, top_audience) = self.get_leaderboards().await;
                                    self.broadcast_to_all(crate::protocol::ServerMessage::Scores {
                                        players: all_players,
                                        audience_top: top_audience.into_iter().take(10).collect(),
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("Failed to compute scores: {}", e);
                                    // Note: Phase transition still succeeds but scores aren't computed
                                    // Error is already bubbled up from precondition check
                                }
                            }
                        } else {
                            // Already scored, just re-broadcast the existing scores
                            let (all_players, top_audience) = self.get_leaderboards().await;
                            self.broadcast_to_all(crate::protocol::ServerMessage::Scores {
                                players: all_players,
                                audience_top: top_audience.into_iter().take(10).collect(),
                            });
                        }
                    }
                } else if effective_phase == GamePhase::Podium {
                    // Re-broadcast scores for audience winner display
                    let (all_players, top_audience) = self.get_leaderboards().await;
                    self.broadcast_to_all(crate::protocol::ServerMessage::Scores {
                        players: all_players,
                        audience_top: top_audience.into_iter().take(10).collect(),
                    });
                }

                // Broadcast phase change to all clients
                self.broadcast_phase_change().await;

                Ok(())
            } else {
                Err("Game was removed during transition".to_string())
            }
        } else {
            Err("No active game".to_string())
        }
    }

    /// Broadcast current phase to all clients
    async fn broadcast_phase_change(&self) {
        if let Some(game) = self.get_game().await {
            let valid_transitions = Self::get_valid_transitions(&game.phase);
            // Include prompt when transitioning to WRITING so all clients have it
            let prompt = if game.phase == GamePhase::Writing {
                self.get_current_round()
                    .await
                    .and_then(|r| r.selected_prompt)
            } else {
                None
            };
            self.broadcast_to_all(crate::protocol::ServerMessage::Phase {
                phase: game.phase.clone(),
                round_no: game.round_no,
                server_now: chrono::Utc::now().to_rfc3339(),
                deadline: game.phase_deadline.clone(),
                valid_transitions,
                prompt,
            });
        }
    }

    /// Reset game to initial state for new volunteers
    /// Note: This preserves the prompt_pool so prompts persist across games
    /// Use clear_prompt_pool() to clear prompts explicitly
    pub async fn reset_game(&self) {
        // Clear game-specific state (NOT prompt_pool - that persists)
        self.players.write().await.clear();
        self.rounds.write().await.clear();
        self.submissions.write().await.clear();
        self.votes.write().await.clear();
        self.scores.write().await.clear();
        self.player_status.write().await.clear();
        self.processed_vote_msg_ids.write().await.clear();
        // Clear queued prompts and prompt votes (move back to pool)
        self.clear_queued_prompts().await;
        self.prompt_votes.write().await.clear();

        // Reset game to initial state
        let mut game = self.game.write().await;
        if let Some(ref mut g) = *game {
            g.phase = GamePhase::Lobby;
            g.round_no = 0;
            g.current_round_id = None;
            g.phase_deadline = None;
            g.panic_mode = false;
            g.version += 1;
        }
        drop(game);

        // Broadcast reset state to all clients
        self.broadcast_phase_change().await;

        tracing::info!("Game reset to initial state (prompt pool preserved)");
    }

    /// Clear the prompt pool (e.g., at end of evening)
    pub async fn clear_prompt_pool(&self) {
        self.prompt_pool.write().await.clear();
        tracing::info!("Prompt pool cleared");
    }

    /// Toggle panic mode on/off
    pub async fn set_panic_mode(&self, enabled: bool) {
        let mut game = self.game.write().await;
        if let Some(ref mut g) = *game {
            g.panic_mode = enabled;
            g.version += 1;
        }
        drop(game);

        // Broadcast panic mode change to all clients
        self.broadcast_to_all(crate::protocol::ServerMessage::PanicModeUpdate { enabled });

        tracing::info!("Panic mode set to: {}", enabled);
    }

    /// Check if panic mode is active
    pub async fn is_panic_mode(&self) -> bool {
        self.game
            .read()
            .await
            .as_ref()
            .map(|g| g.panic_mode)
            .unwrap_or(false)
    }

    /// Extend the current phase deadline by a number of seconds
    pub async fn extend_deadline(&self, seconds: u32) -> Result<String, String> {
        let mut game = self.game.write().await;
        if let Some(ref mut g) = *game {
            if let Some(ref deadline_str) = g.phase_deadline {
                // Parse current deadline
                let current_deadline = chrono::DateTime::parse_from_rfc3339(deadline_str)
                    .map_err(|e| format!("Invalid deadline format: {}", e))?;

                // Add seconds to deadline
                let new_deadline = current_deadline + chrono::Duration::seconds(seconds as i64);
                let new_deadline_str = new_deadline.to_rfc3339();

                g.phase_deadline = Some(new_deadline_str.clone());
                g.version += 1;

                Ok(new_deadline_str)
            } else {
                Err("No active deadline to extend".to_string())
            }
        } else {
            Err("No active game".to_string())
        }
    }

    /// Set manual AI winner for panic mode scoring
    pub async fn set_manual_ai_winner(
        &self,
        round_id: &str,
        submission_id: String,
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
            round.manual_ai_winner = Some(submission_id);
            Ok(())
        } else {
            Err("Round not found".to_string())
        }
    }

    /// Set manual funny winner for panic mode scoring
    pub async fn set_manual_funny_winner(
        &self,
        round_id: &str,
        submission_id: String,
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
            round.manual_funny_winner = Some(submission_id);
            Ok(())
        } else {
            Err("Round not found".to_string())
        }
    }
}
