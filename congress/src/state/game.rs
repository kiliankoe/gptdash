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
}
