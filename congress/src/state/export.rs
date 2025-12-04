//! State export/import for quick and dirty state restoration.
//!
//! This module provides serializable snapshots of the full application state
//! for backup and restoration during live events.

use crate::protocol::PlayerSubmissionStatus;
use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Schema version for export format compatibility
pub const EXPORT_SCHEMA_VERSION: u32 = 1;

/// A serializable snapshot of the entire game state.
///
/// Excludes runtime-only components:
/// - Broadcast channels (recreated at startup)
/// - LlmManager (reconstructed from config)
/// - API keys (security)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStateExport {
    /// Schema version for forward compatibility
    pub schema_version: u32,
    /// Export timestamp (ISO8601)
    pub exported_at: String,
    /// The current game state
    pub game: Option<Game>,
    /// All rounds (keyed by round ID)
    pub rounds: HashMap<RoundId, Round>,
    /// All submissions (keyed by submission ID)
    pub submissions: HashMap<SubmissionId, Submission>,
    /// All votes (keyed by vote ID)
    pub votes: HashMap<VoteId, Vote>,
    /// All players (keyed by player ID)
    pub players: HashMap<PlayerId, Player>,
    /// Computed scores
    pub scores: Vec<Score>,
    /// Vote message ID deduplication state (voter_id -> msg_id)
    pub processed_vote_msg_ids: HashMap<VoterId, String>,
    /// Player submission status tracking
    pub player_status: HashMap<PlayerId, PlayerSubmissionStatus>,
    /// Shadowbanned audience member IDs
    #[serde(default)]
    pub shadowbanned_audience: HashSet<VoterId>,
}

impl GameStateExport {
    /// Create a new export with current timestamp
    pub fn new(
        game: Option<Game>,
        rounds: HashMap<RoundId, Round>,
        submissions: HashMap<SubmissionId, Submission>,
        votes: HashMap<VoteId, Vote>,
        players: HashMap<PlayerId, Player>,
        scores: Vec<Score>,
        processed_vote_msg_ids: HashMap<VoterId, String>,
        player_status: HashMap<PlayerId, PlayerSubmissionStatus>,
        shadowbanned_audience: HashSet<VoterId>,
    ) -> Self {
        Self {
            schema_version: EXPORT_SCHEMA_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            game,
            rounds,
            submissions,
            votes,
            players,
            scores,
            processed_vote_msg_ids,
            player_status,
            shadowbanned_audience,
        }
    }

    /// Validate the export before import
    pub fn validate(&self) -> Result<(), String> {
        // Check schema version
        if self.schema_version > EXPORT_SCHEMA_VERSION {
            return Err(format!(
                "Export schema version {} is newer than supported version {}. \
                 Please update the server.",
                self.schema_version, EXPORT_SCHEMA_VERSION
            ));
        }

        // Basic sanity checks
        if let Some(ref game) = self.game {
            // If there's a current round, it should exist in rounds
            if let Some(ref round_id) = game.current_round_id {
                if !self.rounds.contains_key(round_id) {
                    return Err(format!(
                        "Game references current_round_id '{}' but round not found in export",
                        round_id
                    ));
                }
            }
        }

        // Verify submission round references
        for (sub_id, sub) in &self.submissions {
            if !self.rounds.contains_key(&sub.round_id) {
                return Err(format!(
                    "Submission '{}' references round '{}' which doesn't exist",
                    sub_id, sub.round_id
                ));
            }
        }

        // Verify vote round references
        for (vote_id, vote) in &self.votes {
            if !self.rounds.contains_key(&vote.round_id) {
                return Err(format!(
                    "Vote '{}' references round '{}' which doesn't exist",
                    vote_id, vote.round_id
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_serialization_roundtrip() {
        let export = GameStateExport::new(
            Some(Game {
                id: "game_1".to_string(),
                version: 1,
                phase: GamePhase::Lobby,
                round_no: 0,
                config: GameConfig::default(),
                current_round_id: None,
                phase_deadline: None,
                panic_mode: false,
            }),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            Vec::new(),
            HashMap::new(),
            HashMap::new(),
            HashSet::new(),
        );

        let json = serde_json::to_string_pretty(&export).unwrap();
        let parsed: GameStateExport = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.schema_version, EXPORT_SCHEMA_VERSION);
        assert!(parsed.game.is_some());
    }

    #[test]
    fn test_validation_missing_round() {
        let export = GameStateExport {
            schema_version: EXPORT_SCHEMA_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            game: Some(Game {
                id: "game_1".to_string(),
                version: 1,
                phase: GamePhase::Writing,
                round_no: 1,
                config: GameConfig::default(),
                current_round_id: Some("missing_round".to_string()),
                phase_deadline: None,
                panic_mode: false,
            }),
            rounds: HashMap::new(),
            submissions: HashMap::new(),
            votes: HashMap::new(),
            players: HashMap::new(),
            scores: Vec::new(),
            processed_vote_msg_ids: HashMap::new(),
            player_status: HashMap::new(),
            shadowbanned_audience: HashSet::new(),
        };

        let result = export.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("round not found"));
    }

    #[test]
    fn test_validation_future_schema() {
        let export = GameStateExport {
            schema_version: EXPORT_SCHEMA_VERSION + 1,
            exported_at: chrono::Utc::now().to_rfc3339(),
            game: None,
            rounds: HashMap::new(),
            submissions: HashMap::new(),
            votes: HashMap::new(),
            players: HashMap::new(),
            scores: Vec::new(),
            processed_vote_msg_ids: HashMap::new(),
            player_status: HashMap::new(),
            shadowbanned_audience: HashSet::new(),
        };

        let result = export.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("newer than supported"));
    }
}
