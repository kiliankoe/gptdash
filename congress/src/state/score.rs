use crate::state::AppState;
use crate::types::*;
use std::collections::HashMap;

impl AppState {
    /// Compute scores for a round and add them to the scores collection
    /// Returns (player_scores, audience_scores)
    pub async fn compute_scores(
        &self,
        round_id: &RoundId,
    ) -> Result<(Vec<Score>, Vec<Score>), String> {
        let round = self
            .rounds
            .read()
            .await
            .get(round_id)
            .cloned()
            .ok_or("Round not found")?;

        let ai_submission_id = round
            .ai_submission_id
            .as_ref()
            .ok_or("AI submission not set")?;

        // Get all submissions for this round
        let submissions = self.submissions.read().await;
        let round_submissions: Vec<_> = submissions
            .values()
            .filter(|s| s.round_id == *round_id)
            .collect();

        // Aggregate votes to get counts
        let (ai_counts, funny_counts) = self.aggregate_votes(round_id).await;

        // Compute player scores
        let mut player_scores = Vec::new();
        for submission in round_submissions.iter() {
            if submission.author_kind == AuthorKind::Player {
                if let Some(player_id) = &submission.author_ref {
                    let ai_votes = ai_counts.get(&submission.id).copied().unwrap_or(0);
                    let funny_votes = funny_counts.get(&submission.id).copied().unwrap_or(0);

                    let score = Score {
                        id: ulid::Ulid::new().to_string(),
                        kind: ScoreKind::Player,
                        ref_id: player_id.clone(),
                        ai_detect_points: ai_votes,
                        funny_points: funny_votes,
                        total: ai_votes + funny_votes,
                    };
                    player_scores.push(score);
                }
            }
        }

        // Compute audience scores
        let votes = self.votes.read().await;
        let mut audience_scores_map: HashMap<VoterId, Score> = HashMap::new();

        for vote in votes.values() {
            if vote.round_id == *round_id {
                let score = audience_scores_map
                    .entry(vote.voter_id.clone())
                    .or_insert_with(|| Score {
                        id: ulid::Ulid::new().to_string(),
                        kind: ScoreKind::Audience,
                        ref_id: vote.voter_id.clone(),
                        ai_detect_points: 0,
                        funny_points: 0,
                        total: 0,
                    });

                // +1 if they correctly identified the AI
                if vote.ai_pick_submission_id == *ai_submission_id {
                    score.ai_detect_points += 1;
                    score.total += 1;
                }
            }
        }

        let audience_scores: Vec<Score> = audience_scores_map.into_values().collect();

        // Store scores
        let mut scores = self.scores.write().await;
        scores.extend(player_scores.iter().cloned());
        scores.extend(audience_scores.iter().cloned());

        Ok((player_scores, audience_scores))
    }

    /// Get aggregated scores across all rounds (cumulative leaderboards)
    pub async fn get_leaderboards(&self) -> (Vec<Score>, Vec<Score>) {
        let scores = self.scores.read().await;

        let mut player_totals: HashMap<String, Score> = HashMap::new();
        let mut audience_totals: HashMap<String, Score> = HashMap::new();

        for score in scores.iter() {
            match score.kind {
                ScoreKind::Player => {
                    let entry = player_totals
                        .entry(score.ref_id.clone())
                        .or_insert_with(|| Score {
                            id: score.ref_id.clone(),
                            kind: ScoreKind::Player,
                            ref_id: score.ref_id.clone(),
                            ai_detect_points: 0,
                            funny_points: 0,
                            total: 0,
                        });
                    entry.ai_detect_points += score.ai_detect_points;
                    entry.funny_points += score.funny_points;
                    entry.total += score.total;
                }
                ScoreKind::Audience => {
                    let entry = audience_totals
                        .entry(score.ref_id.clone())
                        .or_insert_with(|| Score {
                            id: score.ref_id.clone(),
                            kind: ScoreKind::Audience,
                            ref_id: score.ref_id.clone(),
                            ai_detect_points: 0,
                            funny_points: 0,
                            total: 0,
                        });
                    entry.ai_detect_points += score.ai_detect_points;
                    entry.funny_points += score.funny_points;
                    entry.total += score.total;
                }
            }
        }

        let mut player_scores: Vec<Score> = player_totals.into_values().collect();
        let mut audience_scores: Vec<Score> = audience_totals.into_values().collect();

        // Sort by total descending
        player_scores.sort_by(|a, b| b.total.cmp(&a.total));
        audience_scores.sort_by(|a, b| b.total.cmp(&a.total));

        (player_scores, audience_scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compute_scores_no_votes() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add AI submission
        let ai_sub = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();
        state
            .rounds
            .write()
            .await
            .get_mut(&round.id)
            .unwrap()
            .ai_submission_id = Some(ai_sub.id.clone());

        let result = state.compute_scores(&round.id).await;
        assert!(result.is_ok());

        let (player_scores, audience_scores) = result.unwrap();
        assert_eq!(player_scores.len(), 0);
        assert_eq!(audience_scores.len(), 0);
    }

    #[tokio::test]
    async fn test_compute_scores_player_gets_ai_votes() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add AI submission and player submission
        let ai_sub = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();
        let player = state.create_player().await;
        let player_sub = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Player answer".to_string(),
            )
            .await
            .unwrap();

        state
            .rounds
            .write()
            .await
            .get_mut(&round.id)
            .unwrap()
            .ai_submission_id = Some(ai_sub.id.clone());

        // Two votes: both pick player's answer as AI (player fooled them)
        let vote1 = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round.id.clone(),
            voter_id: "voter1".to_string(),
            ai_pick_submission_id: player_sub.id.clone(),
            funny_pick_submission_id: ai_sub.id.clone(),
            ts: chrono::Utc::now().to_rfc3339(),
        };
        let vote2 = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round.id.clone(),
            voter_id: "voter2".to_string(),
            ai_pick_submission_id: player_sub.id.clone(),
            funny_pick_submission_id: player_sub.id.clone(),
            ts: chrono::Utc::now().to_rfc3339(),
        };

        state.votes.write().await.insert(vote1.id.clone(), vote1);
        state.votes.write().await.insert(vote2.id.clone(), vote2);

        let (player_scores, audience_scores) = state.compute_scores(&round.id).await.unwrap();

        assert_eq!(player_scores.len(), 1);
        assert_eq!(player_scores[0].ref_id, player.id);
        assert_eq!(player_scores[0].ai_detect_points, 2); // Got 2 AI votes
        assert_eq!(player_scores[0].funny_points, 1); // Got 1 funny vote
        assert_eq!(player_scores[0].total, 3);

        // Both audience members guessed wrong
        assert_eq!(audience_scores.len(), 2);
        assert_eq!(audience_scores[0].ai_detect_points, 0);
        assert_eq!(audience_scores[1].ai_detect_points, 0);
    }

    #[tokio::test]
    async fn test_compute_scores_audience_correct_detection() {
        let state = AppState::new();
        state.create_game().await;
        let round = state.start_round().await.unwrap();

        // Add AI submission
        let ai_sub = state
            .submit_answer(&round.id, None, "AI answer".to_string())
            .await
            .unwrap();
        let player = state.create_player().await;
        let player_sub = state
            .submit_answer(
                &round.id,
                Some(player.id.clone()),
                "Player answer".to_string(),
            )
            .await
            .unwrap();

        state
            .rounds
            .write()
            .await
            .get_mut(&round.id)
            .unwrap()
            .ai_submission_id = Some(ai_sub.id.clone());

        // One vote correctly identifies AI
        let vote = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round.id.clone(),
            voter_id: "voter1".to_string(),
            ai_pick_submission_id: ai_sub.id.clone(),
            funny_pick_submission_id: player_sub.id.clone(),
            ts: chrono::Utc::now().to_rfc3339(),
        };
        state.votes.write().await.insert(vote.id.clone(), vote);

        let (player_scores, audience_scores) = state.compute_scores(&round.id).await.unwrap();

        assert_eq!(player_scores.len(), 1);
        assert_eq!(player_scores[0].ai_detect_points, 0); // No AI votes received

        assert_eq!(audience_scores.len(), 1);
        assert_eq!(audience_scores[0].ai_detect_points, 1); // Correctly identified AI
        assert_eq!(audience_scores[0].total, 1);
    }

    #[tokio::test]
    async fn test_get_leaderboards() {
        let state = AppState::new();
        state.create_game().await;

        // Round 1
        let round1 = state.start_round().await.unwrap();
        let player1 = state.create_player().await;
        let ai_sub1 = state
            .submit_answer(&round1.id, None, "AI".to_string())
            .await
            .unwrap();
        let p1_sub = state
            .submit_answer(&round1.id, Some(player1.id.clone()), "P1".to_string())
            .await
            .unwrap();
        state
            .rounds
            .write()
            .await
            .get_mut(&round1.id)
            .unwrap()
            .ai_submission_id = Some(ai_sub1.id.clone());

        let vote1 = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round1.id.clone(),
            voter_id: "voter1".to_string(),
            ai_pick_submission_id: p1_sub.id.clone(),
            funny_pick_submission_id: p1_sub.id.clone(),
            ts: chrono::Utc::now().to_rfc3339(),
        };
        state.votes.write().await.insert(vote1.id.clone(), vote1);

        state.compute_scores(&round1.id).await.unwrap();

        let (player_scores, _audience_scores) = state.get_leaderboards().await;

        assert_eq!(player_scores.len(), 1);
        assert_eq!(player_scores[0].total, 2); // 1 AI vote + 1 funny vote
    }

    #[tokio::test]
    async fn test_leaderboard_accumulates_across_rounds() {
        let state = AppState::new();
        state.create_game().await;

        let player = state.create_player().await;

        // Round 1: player gets 2 points
        let round1 = state.start_round().await.unwrap();
        let ai_sub1 = state
            .submit_answer(&round1.id, None, "AI1".to_string())
            .await
            .unwrap();
        let p_sub1 = state
            .submit_answer(&round1.id, Some(player.id.clone()), "P1".to_string())
            .await
            .unwrap();
        state
            .rounds
            .write()
            .await
            .get_mut(&round1.id)
            .unwrap()
            .ai_submission_id = Some(ai_sub1.id.clone());

        let vote1 = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round1.id.clone(),
            voter_id: "voter1".to_string(),
            ai_pick_submission_id: p_sub1.id.clone(),
            funny_pick_submission_id: p_sub1.id.clone(),
            ts: chrono::Utc::now().to_rfc3339(),
        };
        state.votes.write().await.insert(vote1.id.clone(), vote1);
        state.compute_scores(&round1.id).await.unwrap();

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

        // Round 2: player gets 1 more point
        let ai_sub2 = state
            .submit_answer(&round2.id, None, "AI2".to_string())
            .await
            .unwrap();
        let p_sub2 = state
            .submit_answer(&round2.id, Some(player.id.clone()), "P2".to_string())
            .await
            .unwrap();
        state
            .rounds
            .write()
            .await
            .get_mut(&round2.id)
            .unwrap()
            .ai_submission_id = Some(ai_sub2.id.clone());

        let vote2 = Vote {
            id: ulid::Ulid::new().to_string(),
            round_id: round2.id.clone(),
            voter_id: "voter2".to_string(),
            ai_pick_submission_id: p_sub2.id.clone(),
            funny_pick_submission_id: ai_sub2.id.clone(),
            ts: chrono::Utc::now().to_rfc3339(),
        };
        state.votes.write().await.insert(vote2.id.clone(), vote2);
        state.compute_scores(&round2.id).await.unwrap();

        let (player_scores, _audience_scores) = state.get_leaderboards().await;

        assert_eq!(player_scores.len(), 1);
        assert_eq!(player_scores[0].total, 3); // 2 from round1 + 1 from round2
    }
}
