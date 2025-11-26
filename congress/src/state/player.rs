use super::AppState;
use crate::types::*;

impl AppState {
    /// Create a new player with a token
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
    pub async fn register_player(
        &self,
        token: &str,
        display_name: String,
    ) -> Result<Player, String> {
        let mut players = self.players.write().await;

        if let Some(player) = players.values_mut().find(|p| p.token == token) {
            player.display_name = Some(display_name.clone());
            Ok(player.clone())
        } else {
            Err("Invalid player token".to_string())
        }
    }

    /// Get player by token
    pub async fn get_player_by_token(&self, token: &str) -> Option<Player> {
        self.players
            .read()
            .await
            .values()
            .find(|p| p.token == token)
            .cloned()
    }

    /// Get a player's submission for the current round
    pub async fn get_player_submission_for_current_round(
        &self,
        player_id: &PlayerId,
    ) -> Option<Submission> {
        let round = self.get_current_round().await?;
        let submissions = self.submissions.read().await;

        submissions
            .values()
            .find(|s| {
                s.round_id == round.id
                    && s.author_kind == AuthorKind::Player
                    && s.author_ref.as_ref() == Some(player_id)
            })
            .cloned()
    }

    /// Get an audience member's vote for the current round
    pub async fn get_audience_vote_for_current_round(&self, voter_id: &VoterId) -> Option<Vote> {
        let round = self.get_current_round().await?;
        let votes = self.votes.read().await;

        votes
            .values()
            .find(|v| v.round_id == round.id && v.voter_id == *voter_id)
            .cloned()
    }
}
