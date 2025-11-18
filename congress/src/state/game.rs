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
        use GamePhase::*;

        match (from, to) {
            // Normal forward flow
            (Lobby, PromptSelection) => true,
            (PromptSelection, Writing) => true,
            (Writing, Reveal) => true,
            (Reveal, Voting) => true,
            (Voting, Results) => true,

            // From Results can go to multiple places
            (Results, Podium) => true,
            (Results, PromptSelection) => true, // Next round
            (Results, Intermission) => true,

            // From Podium
            (Podium, PromptSelection) => true, // New round/new volunteers
            (Podium, Ended) => true,

            // Intermission can return to any phase except Ended
            (Intermission, Ended) => true,
            (Intermission, _) => true,

            // Any phase can go to Intermission (panic mode) or Ended (hard stop)
            (_, Intermission) => true,
            (_, Ended) => true,

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Validate preconditions for a phase transition
    async fn validate_phase_preconditions(
        &self,
        game: &Game,
        to: &GamePhase,
    ) -> Result<(), String> {
        match to {
            GamePhase::Writing => {
                // Requires a current round with selected prompt
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

            // Re-acquire lock and apply transition
            let mut game = self.game.write().await;
            if let Some(ref mut g) = *game {
                g.phase = new_phase.clone();
                g.version += 1;

                let round_id = g.current_round_id.clone();
                drop(game);

                // Handle phase-specific actions
                if new_phase == GamePhase::Results {
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
            let round = self.get_current_round().await;
            let deadline = round.and_then(|r| r.submission_deadline);

            self.broadcast_to_all(crate::protocol::ServerMessage::Phase {
                phase: game.phase,
                round_no: game.round_no,
                server_now: chrono::Utc::now().to_rfc3339(),
                deadline,
            });
        }
    }
}
