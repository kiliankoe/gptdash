use crate::state::AppState;
use crate::types::*;
use std::collections::HashMap;

/// Result of submitting a vote
#[derive(Debug, PartialEq)]
pub enum VoteResult {
    /// Vote was recorded (new vote or updated existing)
    Recorded,
    /// Vote was a duplicate (same msg_id already processed)
    Duplicate,
    /// No active round to vote in
    NoActiveRound,
}

impl AppState {
    /// Submit a vote with idempotency support
    /// Returns whether the vote was recorded, was a duplicate, or failed
    pub async fn submit_vote(
        &self,
        voter_id: VoterId,
        ai_pick: SubmissionId,
        funny_pick: SubmissionId,
        msg_id: String,
    ) -> VoteResult {
        // Get current round
        let round = match self.get_current_round().await {
            Some(r) => r,
            None => return VoteResult::NoActiveRound,
        };

        // Check for duplicate msg_id (idempotency)
        {
            let processed = self.processed_vote_msg_ids.read().await;
            if let Some(last_msg_id) = processed.get(&voter_id) {
                if *last_msg_id == msg_id {
                    tracing::debug!("Duplicate vote msg_id {} from voter {}", msg_id, voter_id);
                    return VoteResult::Duplicate;
                }
            }
        }

        // Find and remove any existing vote from this voter for this round
        {
            let mut votes = self.votes.write().await;
            let existing_vote_id = votes
                .iter()
                .find(|(_, v)| v.voter_id == voter_id && v.round_id == round.id)
                .map(|(id, _)| id.clone());

            if let Some(vote_id) = existing_vote_id {
                votes.remove(&vote_id);
            }
        }

        // Create and store new vote
        let vote = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round.id,
            voter_id: voter_id.clone(),
            ai_pick_submission_id: ai_pick,
            funny_pick_submission_id: funny_pick,
            ts: chrono::Utc::now().to_rfc3339(),
        };

        self.votes.write().await.insert(vote.id.clone(), vote);

        // Record the msg_id as processed
        self.processed_vote_msg_ids
            .write()
            .await
            .insert(voter_id, msg_id);

        VoteResult::Recorded
    }

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

    #[tokio::test]
    async fn test_submit_vote_records_new_vote() {
        let state = AppState::new();
        state.create_game().await;
        state.start_round().await.unwrap();

        let result = state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::Recorded);
        assert_eq!(state.votes.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_submit_vote_duplicate_msg_id() {
        let state = AppState::new();
        state.create_game().await;
        state.start_round().await.unwrap();

        // First vote
        let result1 = state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;
        assert_eq!(result1, VoteResult::Recorded);

        // Same msg_id - should be duplicate
        let result2 = state
            .submit_vote(
                "voter1".to_string(),
                "sub3".to_string(),
                "sub4".to_string(),
                "msg1".to_string(),
            )
            .await;
        assert_eq!(result2, VoteResult::Duplicate);

        // Vote should not have changed
        assert_eq!(state.votes.read().await.len(), 1);
        let vote = state.votes.read().await.values().next().unwrap().clone();
        assert_eq!(vote.ai_pick_submission_id, "sub1");
    }

    #[tokio::test]
    async fn test_submit_vote_new_msg_id_replaces_vote() {
        let state = AppState::new();
        state.create_game().await;
        state.start_round().await.unwrap();

        // First vote
        state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;

        // New msg_id - should replace
        let result = state
            .submit_vote(
                "voter1".to_string(),
                "sub3".to_string(),
                "sub4".to_string(),
                "msg2".to_string(),
            )
            .await;
        assert_eq!(result, VoteResult::Recorded);

        // Should still only have 1 vote, but updated
        assert_eq!(state.votes.read().await.len(), 1);
        let vote = state.votes.read().await.values().next().unwrap().clone();
        assert_eq!(vote.ai_pick_submission_id, "sub3");
        assert_eq!(vote.funny_pick_submission_id, "sub4");
    }

    #[tokio::test]
    async fn test_submit_vote_no_active_round() {
        let state = AppState::new();
        state.create_game().await;
        // Don't start a round

        let result = state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::NoActiveRound);
    }
}
