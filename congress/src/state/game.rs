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
                g.phase = new_phase;
                g.version += 1;
                Ok(())
            } else {
                Err("Game was removed during transition".to_string())
            }
        } else {
            Err("No active game".to_string())
        }
    }
}
