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
    /// Panic mode is active, voting disabled
    PanicModeActive,
    /// Game is not in VOTING phase
    WrongPhase,
    /// Vote is invalid (e.g., picks don't exist for this round)
    InvalidPick,
    /// Voter token not found in audience_members (possible fabricated identity)
    UnknownVoter,
    /// Voter has already cast a vote this round (only one vote allowed)
    AlreadyVoted,
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
        // Validate voter exists in audience_members (prevent fabricated tokens)
        {
            let members = self.audience_members.read().await;
            if !members.contains_key(&voter_id) {
                tracing::warn!("Vote rejected: unknown voter_id {}", voter_id);
                return VoteResult::UnknownVoter;
            }
        }

        // Check game phase and panic mode
        {
            let game = self.game.read().await;
            if let Some(ref g) = *game {
                // Only accept votes during VOTING phase
                if g.phase != GamePhase::Voting {
                    tracing::warn!("Vote rejected: wrong phase {:?} (expected VOTING)", g.phase);
                    return VoteResult::WrongPhase;
                }
                if g.panic_mode {
                    return VoteResult::PanicModeActive;
                }
            } else {
                return VoteResult::NoActiveRound;
            }
        }

        // Get current round
        let round = match self.get_current_round().await {
            Some(r) => r,
            None => return VoteResult::NoActiveRound,
        };

        // Validate vote content: picks must exist and belong to the current round.
        {
            let submissions = self.submissions.read().await;
            let ai_ok = submissions
                .get(&ai_pick)
                .is_some_and(|s| s.round_id == round.id);
            let funny_ok = submissions
                .get(&funny_pick)
                .is_some_and(|s| s.round_id == round.id);
            if !ai_ok || !funny_ok {
                tracing::warn!(
                    "Vote rejected: invalid pick(s) (ai_ok={}, funny_ok={})",
                    ai_ok,
                    funny_ok
                );
                return VoteResult::InvalidPick;
            }
        }

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

        // Acquire locks for atomic check-and-store to prevent TOCTOU race conditions.
        // We need to hold both submissions (read) and votes (write) locks together
        // to ensure submissions aren't removed between validation and vote storage.
        let submissions = self.submissions.read().await;
        let mut votes = self.votes.write().await;

        // Re-validate submissions still exist (they could have been removed since initial check)
        let ai_ok = submissions
            .get(&ai_pick)
            .is_some_and(|s| s.round_id == round.id);
        let funny_ok = submissions
            .get(&funny_pick)
            .is_some_and(|s| s.round_id == round.id);
        if !ai_ok || !funny_ok {
            tracing::warn!(
                "Vote rejected: pick(s) no longer valid (ai_ok={}, funny_ok={})",
                ai_ok,
                funny_ok
            );
            return VoteResult::InvalidPick;
        }

        // Check if voter has already voted this round (only one vote allowed)
        let already_voted = votes
            .iter()
            .any(|(_, v)| v.voter_id == voter_id && v.round_id == round.id);

        if already_voted {
            tracing::info!(
                "Vote rejected: voter {} already voted in round {}",
                voter_id,
                round.id
            );
            return VoteResult::AlreadyVoted;
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

        votes.insert(vote.id.clone(), vote);

        // Drop locks before acquiring new ones
        drop(votes);
        drop(submissions);

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

    /// Get vote counts for a specific round (alias for aggregate_votes for clarity)
    pub async fn get_vote_counts_for_round(
        &self,
        round_id: &RoundId,
    ) -> (HashMap<SubmissionId, u32>, HashMap<SubmissionId, u32>) {
        self.aggregate_votes(round_id).await
    }

    /// Get a voter's prompt vote for the current prompt selection phase
    pub async fn get_audience_prompt_vote(&self, voter_id: &str) -> Option<String> {
        self.prompt_votes.read().await.get(voter_id).cloned()
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

    /// Helper to set up state in VOTING phase with a test voter
    async fn setup_voting_phase() -> AppState {
        let state = AppState::new();
        state.create_game().await;
        state.start_round().await.unwrap();
        // Set game to VOTING phase
        state.game.write().await.as_mut().unwrap().phase = GamePhase::Voting;
        // Create a test audience member (required for voter validation)
        state.get_or_create_audience_member("voter1").await;
        state
    }

    #[tokio::test]
    async fn test_submit_vote_records_new_vote() {
        let state = setup_voting_phase().await;

        let round = state.get_current_round().await.unwrap();
        let sub1 = state
            .submit_answer(&round.id, None, "A".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, None, "B".to_string())
            .await
            .unwrap();

        let result = state
            .submit_vote("voter1".to_string(), sub1.id, sub2.id, "msg1".to_string())
            .await;

        assert_eq!(result, VoteResult::Recorded);
        assert_eq!(state.votes.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_submit_vote_duplicate_msg_id() {
        let state = setup_voting_phase().await;

        let round = state.get_current_round().await.unwrap();
        let sub1 = state
            .submit_answer(&round.id, None, "A".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, None, "B".to_string())
            .await
            .unwrap();
        let sub3 = state
            .submit_answer(&round.id, None, "C".to_string())
            .await
            .unwrap();
        let sub4 = state
            .submit_answer(&round.id, None, "D".to_string())
            .await
            .unwrap();

        // First vote
        let result1 = state
            .submit_vote(
                "voter1".to_string(),
                sub1.id.clone(),
                sub2.id.clone(),
                "msg1".to_string(),
            )
            .await;
        assert_eq!(result1, VoteResult::Recorded);

        // Same msg_id - should be duplicate
        let result2 = state
            .submit_vote("voter1".to_string(), sub3.id, sub4.id, "msg1".to_string())
            .await;
        assert_eq!(result2, VoteResult::Duplicate);

        // Vote should not have changed
        assert_eq!(state.votes.read().await.len(), 1);
        let vote = state.votes.read().await.values().next().unwrap().clone();
        assert_eq!(vote.ai_pick_submission_id, sub1.id);
    }

    #[tokio::test]
    async fn test_submit_vote_second_vote_rejected() {
        let state = setup_voting_phase().await;

        let round = state.get_current_round().await.unwrap();
        let sub1 = state
            .submit_answer(&round.id, None, "A".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, None, "B".to_string())
            .await
            .unwrap();
        let sub3 = state
            .submit_answer(&round.id, None, "C".to_string())
            .await
            .unwrap();
        let sub4 = state
            .submit_answer(&round.id, None, "D".to_string())
            .await
            .unwrap();

        // First vote
        let result1 = state
            .submit_vote(
                "voter1".to_string(),
                sub1.id.clone(),
                sub2.id.clone(),
                "msg1".to_string(),
            )
            .await;
        assert_eq!(result1, VoteResult::Recorded);

        // Second vote with new msg_id - should be rejected (only one vote allowed)
        let result2 = state
            .submit_vote(
                "voter1".to_string(),
                sub3.id.clone(),
                sub4.id.clone(),
                "msg2".to_string(),
            )
            .await;
        assert_eq!(result2, VoteResult::AlreadyVoted);

        // Should still only have 1 vote, unchanged
        assert_eq!(state.votes.read().await.len(), 1);
        let vote = state.votes.read().await.values().next().unwrap().clone();
        assert_eq!(vote.ai_pick_submission_id, sub1.id);
        assert_eq!(vote.funny_pick_submission_id, sub2.id);
    }

    #[tokio::test]
    async fn test_submit_vote_no_active_round() {
        let state = AppState::new();
        state.create_game().await;
        // Create audience member (required for voter validation)
        state.get_or_create_audience_member("voter1").await;
        // Set phase to VOTING but don't start a round
        state.game.write().await.as_mut().unwrap().phase = GamePhase::Voting;

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

    #[tokio::test]
    async fn test_submit_vote_rejected_during_lobby_phase() {
        let state = AppState::new();
        state.create_game().await;
        state.start_round().await.unwrap();
        // Create audience member (required for voter validation)
        state.get_or_create_audience_member("voter1").await;
        // Game starts in LOBBY phase by default

        let result = state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::WrongPhase);
        assert_eq!(state.votes.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_submit_vote_rejected_during_writing_phase() {
        let state = AppState::new();
        state.create_game().await;
        state.start_round().await.unwrap();
        // Create audience member (required for voter validation)
        state.get_or_create_audience_member("voter1").await;
        state.game.write().await.as_mut().unwrap().phase = GamePhase::Writing;

        let result = state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::WrongPhase);
        assert_eq!(state.votes.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_submit_vote_rejected_during_reveal_phase() {
        let state = AppState::new();
        state.create_game().await;
        state.start_round().await.unwrap();
        // Create audience member (required for voter validation)
        state.get_or_create_audience_member("voter1").await;
        state.game.write().await.as_mut().unwrap().phase = GamePhase::Reveal;

        let result = state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::WrongPhase);
        assert_eq!(state.votes.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_submit_vote_rejected_during_results_phase() {
        let state = AppState::new();
        state.create_game().await;
        state.start_round().await.unwrap();
        // Create audience member (required for voter validation)
        state.get_or_create_audience_member("voter1").await;
        state.game.write().await.as_mut().unwrap().phase = GamePhase::Results;

        let result = state
            .submit_vote(
                "voter1".to_string(),
                "sub1".to_string(),
                "sub2".to_string(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::WrongPhase);
        assert_eq!(state.votes.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_submit_vote_allows_same_submission_for_both_categories() {
        let state = setup_voting_phase().await;

        // Add a submission to the current round so the pick exists
        let round = state.get_current_round().await.unwrap();
        let sub = state
            .submit_answer(&round.id, None, "AI".to_string())
            .await
            .unwrap();

        let result = state
            .submit_vote(
                "voter1".to_string(),
                sub.id.clone(),
                sub.id.clone(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::Recorded);
        assert_eq!(state.votes.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_submit_vote_rejected_when_picks_not_in_current_round() {
        let state = setup_voting_phase().await;

        let result = state
            .submit_vote(
                "voter1".to_string(),
                "nonexistent1".to_string(),
                "nonexistent2".to_string(),
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::InvalidPick);
        assert_eq!(state.votes.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_submit_vote_rejected_for_unknown_voter() {
        let state = setup_voting_phase().await;

        let round = state.get_current_round().await.unwrap();
        let sub1 = state
            .submit_answer(&round.id, None, "A".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, None, "B".to_string())
            .await
            .unwrap();

        // Try to vote with a fabricated voter token (not in audience_members)
        let result = state
            .submit_vote(
                "fabricated_voter".to_string(),
                sub1.id,
                sub2.id,
                "msg1".to_string(),
            )
            .await;

        assert_eq!(result, VoteResult::UnknownVoter);
        assert_eq!(state.votes.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_multiple_voters_can_each_vote_once() {
        let state = setup_voting_phase().await;
        // Create additional voters
        state.get_or_create_audience_member("voter2").await;
        state.get_or_create_audience_member("voter3").await;

        let round = state.get_current_round().await.unwrap();
        let sub1 = state
            .submit_answer(&round.id, None, "A".to_string())
            .await
            .unwrap();
        let sub2 = state
            .submit_answer(&round.id, None, "B".to_string())
            .await
            .unwrap();

        // All three voters vote
        let r1 = state
            .submit_vote(
                "voter1".to_string(),
                sub1.id.clone(),
                sub2.id.clone(),
                "msg1".to_string(),
            )
            .await;
        let r2 = state
            .submit_vote(
                "voter2".to_string(),
                sub1.id.clone(),
                sub2.id.clone(),
                "msg2".to_string(),
            )
            .await;
        let r3 = state
            .submit_vote(
                "voter3".to_string(),
                sub2.id.clone(),
                sub1.id.clone(),
                "msg3".to_string(),
            )
            .await;

        assert_eq!(r1, VoteResult::Recorded);
        assert_eq!(r2, VoteResult::Recorded);
        assert_eq!(r3, VoteResult::Recorded);
        assert_eq!(state.votes.read().await.len(), 3);
    }
}
