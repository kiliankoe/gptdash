use crate::state::AppState;
use crate::types::*;
use std::collections::HashMap;

impl AppState {
    /// Aggregate votes for a round, returning counts for AI picks and funny picks
    pub async fn aggregate_votes(
        &self,
        round_id: &RoundId,
    ) -> (HashMap<SubmissionId, u32>, HashMap<SubmissionId, u32>) {
        let votes = self.votes.read().await;

        let mut ai_counts: HashMap<SubmissionId, u32> = HashMap::new();
        let mut funny_counts: HashMap<SubmissionId, u32> = HashMap::new();

        for vote in votes.values() {
            if vote.round_id == *round_id {
                *ai_counts
                    .entry(vote.ai_pick_submission_id.clone())
                    .or_insert(0) += 1;
                *funny_counts
                    .entry(vote.funny_pick_submission_id.clone())
                    .or_insert(0) += 1;
            }
        }

        (ai_counts, funny_counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_aggregate_votes_empty() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        let (ai_counts, funny_counts) = state.aggregate_votes(&round.id).await;

        assert!(ai_counts.is_empty());
        assert!(funny_counts.is_empty());
    }

    #[tokio::test]
    async fn test_aggregate_votes_single_vote() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        let vote = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round.id.clone(),
            voter_id: "voter1".to_string(),
            ai_pick_submission_id: "sub1".to_string(),
            funny_pick_submission_id: "sub2".to_string(),
            ts: chrono::Utc::now().to_rfc3339(),
        };
        state.votes.write().await.insert(vote.id.clone(), vote);

        let (ai_counts, funny_counts) = state.aggregate_votes(&round.id).await;

        assert_eq!(ai_counts.get("sub1"), Some(&1));
        assert_eq!(funny_counts.get("sub2"), Some(&1));
    }

    #[tokio::test]
    async fn test_aggregate_votes_multiple_votes() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Three voters, sub1 gets 2 AI votes, sub2 gets 1
        let votes = vec![
            Vote {
                id: ulid::Ulid::new().to_string(),
                round_id: round.id.clone(),
                voter_id: "voter1".to_string(),
                ai_pick_submission_id: "sub1".to_string(),
                funny_pick_submission_id: "sub2".to_string(),
                ts: chrono::Utc::now().to_rfc3339(),
            },
            Vote {
                id: ulid::Ulid::new().to_string(),
                round_id: round.id.clone(),
                voter_id: "voter2".to_string(),
                ai_pick_submission_id: "sub1".to_string(),
                funny_pick_submission_id: "sub3".to_string(),
                ts: chrono::Utc::now().to_rfc3339(),
            },
            Vote {
                id: ulid::Ulid::new().to_string(),
                round_id: round.id.clone(),
                voter_id: "voter3".to_string(),
                ai_pick_submission_id: "sub2".to_string(),
                funny_pick_submission_id: "sub1".to_string(),
                ts: chrono::Utc::now().to_rfc3339(),
            },
        ];

        for vote in votes {
            state.votes.write().await.insert(vote.id.clone(), vote);
        }

        let (ai_counts, funny_counts) = state.aggregate_votes(&round.id).await;

        assert_eq!(ai_counts.get("sub1"), Some(&2));
        assert_eq!(ai_counts.get("sub2"), Some(&1));
        assert_eq!(funny_counts.get("sub1"), Some(&1));
        assert_eq!(funny_counts.get("sub2"), Some(&1));
        assert_eq!(funny_counts.get("sub3"), Some(&1));
    }

    #[tokio::test]
    async fn test_aggregate_votes_ignores_other_rounds() {
        let state = AppState::new();
        state.create_game().await;
        let round1 = state.start_round().await.unwrap();

        // Close round1 and start round2
        state
            .rounds
            .write()
            .await
            .get_mut(&round1.id)
            .unwrap()
            .state = RoundState::Closed;
        state.game.write().await.as_mut().unwrap().phase = GamePhase::Results;
        let round2 = state.start_round().await.unwrap();

        // Add votes to both rounds
        let vote1 = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round1.id.clone(),
            voter_id: "voter1".to_string(),
            ai_pick_submission_id: "sub1".to_string(),
            funny_pick_submission_id: "sub1".to_string(),
            ts: chrono::Utc::now().to_rfc3339(),
        };

        let vote2 = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round2.id.clone(),
            voter_id: "voter2".to_string(),
            ai_pick_submission_id: "sub2".to_string(),
            funny_pick_submission_id: "sub2".to_string(),
            ts: chrono::Utc::now().to_rfc3339(),
        };

        state.votes.write().await.insert(vote1.id.clone(), vote1);
        state.votes.write().await.insert(vote2.id.clone(), vote2);

        // Aggregate for round2 should only include round2 votes
        let (ai_counts, funny_counts) = state.aggregate_votes(&round2.id).await;

        assert_eq!(ai_counts.len(), 1);
        assert_eq!(ai_counts.get("sub2"), Some(&1));
        assert_eq!(funny_counts.len(), 1);
        assert_eq!(funny_counts.get("sub2"), Some(&1));
        assert_eq!(ai_counts.get("sub1"), None);
    }
}
