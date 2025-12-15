use super::AppState;
use crate::types::{AuthorKind, GamePhase, PlayerId, Round, RoundState, Submission, SubmissionId};
use std::collections::HashSet;

/// Normalize text for duplicate comparison (trim whitespace, lowercase)
fn normalize(text: &str) -> String {
    text.trim().to_lowercase()
}

fn clamp_reveal_index(round: &mut Round) {
    if round.reveal_order.is_empty() {
        round.reveal_index = 0;
        return;
    }

    if round.reveal_index >= round.reveal_order.len() {
        round.reveal_index = round.reveal_order.len() - 1;
    }
}

impl AppState {
    /// Submit an answer with automatic exact duplicate detection
    /// If the player already has a submission for this round, it will be replaced
    pub async fn submit_answer(
        &self,
        round_id: &str,
        player_id: Option<String>,
        text: String,
    ) -> Result<Submission, String> {
        // Check for exact duplicates (excluding this player's own submission)
        let normalized_new = normalize(&text);
        let existing = self.get_submissions(round_id).await;
        for existing_sub in &existing {
            // Skip duplicate check against player's own previous submission
            if let Some(ref pid) = player_id {
                if existing_sub.author_ref.as_ref() == Some(pid) {
                    continue;
                }
            }
            if normalize(&existing_sub.original_text) == normalized_new {
                return Err("DUPLICATE_EXACT".to_string());
            }
        }

        // Remove any existing submission from this player for this round
        if let Some(ref pid) = player_id {
            let mut submissions = self.submissions.write().await;
            let to_remove: Vec<String> = submissions
                .values()
                .filter(|s| {
                    s.round_id == round_id
                        && s.author_kind == AuthorKind::Player
                        && s.author_ref.as_ref() == Some(pid)
                })
                .map(|s| s.id.clone())
                .collect();
            for id in to_remove {
                submissions.remove(&id);
            }
        }

        let submission = Submission {
            id: ulid::Ulid::new().to_string(),
            round_id: round_id.to_string(),
            author_kind: if player_id.is_some() {
                AuthorKind::Player
            } else {
                AuthorKind::Ai
            },
            author_ref: player_id,
            original_text: text.clone(),
            display_text: text,
            edited_by_host: Some(false),
            tts_asset_url: None,
        };

        self.submissions
            .write()
            .await
            .insert(submission.id.clone(), submission.clone());

        // Broadcast updated submissions list
        self.broadcast_submissions(round_id).await;

        Ok(submission)
    }

    /// Edit a submission (host only)
    pub async fn edit_submission(
        &self,
        submission_id: &str,
        new_text: String,
    ) -> Result<(), String> {
        let round_id = {
            let mut submissions = self.submissions.write().await;
            if let Some(submission) = submissions.get_mut(submission_id) {
                submission.display_text = new_text;
                submission.edited_by_host = Some(true);
                submission.round_id.clone()
            } else {
                return Err("Submission not found".to_string());
            }
        };

        // Broadcast updated submissions list
        self.broadcast_submissions(&round_id).await;

        Ok(())
    }

    /// Broadcast submissions list for a round
    /// During VOTING/RESULTS/PODIUM: broadcasts to all clients (audience needs to vote/see results)
    /// During other phases: does not broadcast the public submissions list (host gets HostSubmissions)
    pub async fn broadcast_submissions(&self, round_id: &str) {
        let submissions = self.get_submissions(round_id).await;
        tracing::info!(
            "Broadcasting {} submissions for round {}",
            submissions.len(),
            round_id
        );

        // Check current phase to determine broadcast scope
        let phase = self.game.read().await.as_ref().map(|g| g.phase.clone());

        // Public info (no author_kind to prevent spoilers)
        let public_infos: Vec<crate::protocol::SubmissionInfo> =
            submissions.iter().map(|s| s.into()).collect();

        match phase {
            // Audience needs submissions for voting or viewing results
            Some(GamePhase::Voting) | Some(GamePhase::Results) | Some(GamePhase::Podium) => {
                self.broadcast_to_all(crate::protocol::ServerMessage::Submissions {
                    list: public_infos,
                });
            }
            // Outside of voting/results, avoid broadcasting the full submissions list publicly
            // (including to the beamer role) to prevent leaks/spoilers.
            _ => {
                self.broadcast_to_beamer(crate::protocol::ServerMessage::SubmissionCount {
                    count: submissions.len() as u32,
                });
            }
        }

        // Host-only broadcast (includes author_kind for managing the game)
        let host_infos: Vec<crate::protocol::HostSubmissionInfo> =
            submissions.iter().map(|s| s.into()).collect();
        tracing::info!(
            "Sending HostSubmissions to host with {} items",
            host_infos.len()
        );
        self.broadcast_to_host(crate::protocol::ServerMessage::HostSubmissions {
            list: host_infos,
        });
    }

    /// Get submissions for a round
    pub async fn get_submissions(&self, round_id: &str) -> Vec<Submission> {
        self.submissions
            .read()
            .await
            .values()
            .filter(|s| s.round_id == round_id)
            .cloned()
            .collect()
    }

    /// Mark a submission as duplicate and remove it (host only)
    /// Returns the player_id if it was a player submission, so we can notify them
    pub async fn mark_submission_duplicate(
        &self,
        submission_id: &str,
    ) -> Result<Option<String>, String> {
        let (round_id, player_id) = {
            let mut submissions = self.submissions.write().await;
            if let Some(submission) = submissions.remove(submission_id) {
                let player_id = if submission.author_kind == AuthorKind::Player {
                    submission.author_ref.clone()
                } else {
                    None
                };
                (submission.round_id, player_id)
            } else {
                return Err("Submission not found".to_string());
            }
        };

        // Clean up any round references to the removed submission
        let removed_ids: HashSet<SubmissionId> = [submission_id.to_string()].into_iter().collect();
        self.cleanup_round_after_submission_removals(&round_id, &removed_ids)
            .await;

        // Broadcast updated submissions list
        self.broadcast_submissions(&round_id).await;

        Ok(player_id)
    }

    /// Remove a submission from the current round (host only).
    /// Returns the player_id if it was a player submission, so we can notify them.
    pub async fn remove_submission(&self, submission_id: &str) -> Result<Option<PlayerId>, String> {
        let round = self
            .get_current_round()
            .await
            .ok_or_else(|| "No active round".to_string())?;

        // Validate the submission exists and belongs to the current round
        let belongs_to_round = {
            let submissions = self.submissions.read().await;
            submissions
                .get(submission_id)
                .map(|s| s.round_id == round.id)
                .unwrap_or(false)
        };

        if !belongs_to_round {
            return Err("Submission not found in current round".to_string());
        }

        // Removing submissions once voting has started can strand audience clients in "already voted"
        // UI state. Keep this constrained to pre-voting phases.
        if !matches!(
            round.state,
            RoundState::Setup | RoundState::Collecting | RoundState::Revealing
        ) {
            return Err(format!(
                "Can only remove submissions during setup/collecting/revealing (currently: {:?})",
                round.state
            ));
        }

        // Don't allow removal if any votes already exist (they would be invalidated).
        let has_votes = {
            let votes = self.votes.read().await;
            votes.values().any(|v| v.round_id == round.id)
        };
        if has_votes {
            return Err("Cannot remove submissions after votes have been cast".to_string());
        }

        // Don't allow removing the last submission while revealing.
        if round.state == RoundState::Revealing {
            let submission_count = {
                let submissions = self.submissions.read().await;
                submissions
                    .values()
                    .filter(|s| s.round_id == round.id)
                    .count()
            };
            if submission_count <= 1 {
                return Err("Cannot remove the last submission during reveal".to_string());
            }
        }

        let (round_id, player_id) = {
            let mut submissions = self.submissions.write().await;
            let submission = submissions
                .remove(submission_id)
                .ok_or_else(|| "Submission not found".to_string())?;

            let player_id = if submission.author_kind == AuthorKind::Player {
                submission.author_ref.clone()
            } else {
                None
            };

            (submission.round_id, player_id)
        };

        let removed_ids: HashSet<SubmissionId> = [submission_id.to_string()].into_iter().collect();
        self.cleanup_round_after_submission_removals(&round_id, &removed_ids)
            .await;

        // Broadcast updated submissions list
        self.broadcast_submissions(&round_id).await;

        Ok(player_id)
    }

    pub(super) async fn cleanup_round_after_submission_removals(
        &self,
        round_id: &str,
        removed_ids: &HashSet<SubmissionId>,
    ) {
        let mut rounds = self.rounds.write().await;
        let Some(round) = rounds.get_mut(round_id) else {
            return;
        };

        round.reveal_order.retain(|id| !removed_ids.contains(id));
        clamp_reveal_index(round);

        if round
            .ai_submission_id
            .as_ref()
            .is_some_and(|id| removed_ids.contains(id))
        {
            round.ai_submission_id = None;
        }
        if round
            .manual_ai_winner
            .as_ref()
            .is_some_and(|id| removed_ids.contains(id))
        {
            round.manual_ai_winner = None;
        }
        if round
            .manual_funny_winner
            .as_ref()
            .is_some_and(|id| removed_ids.contains(id))
        {
            round.manual_funny_winner = None;
        }
    }

    /// Create a manual AI submission (host override when LLM fails)
    pub async fn create_manual_ai_submission(
        &self,
        round_id: &str,
        text: String,
    ) -> Result<Submission, String> {
        // Validate round exists
        let rounds = self.rounds.read().await;
        if !rounds.contains_key(round_id) {
            return Err("Round not found".to_string());
        }
        drop(rounds);

        let submission = Submission {
            id: ulid::Ulid::new().to_string(),
            round_id: round_id.to_string(),
            author_kind: AuthorKind::Ai,
            author_ref: Some("host:manual".to_string()),
            original_text: text.clone(),
            display_text: text,
            edited_by_host: Some(true),
            tts_asset_url: None,
        };

        self.submissions
            .write()
            .await
            .insert(submission.id.clone(), submission.clone());

        Ok(submission)
    }

    /// Update a player's submission (after typo correction acceptance)
    /// Only the owning player can update their own submission
    pub async fn update_player_submission(
        &self,
        submission_id: &str,
        player_id: &str,
        new_text: String,
    ) -> Result<(), String> {
        // Check for exact duplicates with the new text
        let round_id = {
            let submissions = self.submissions.read().await;
            let submission = submissions
                .get(submission_id)
                .ok_or_else(|| "Submission not found".to_string())?;

            // Verify player owns this submission
            if submission.author_kind != AuthorKind::Player {
                return Err("Cannot update non-player submission".to_string());
            }
            if submission.author_ref.as_ref() != Some(&player_id.to_string()) {
                return Err("Not authorized to update this submission".to_string());
            }

            submission.round_id.clone()
        };

        // Check for duplicates with the new text
        let normalized_new = normalize(&new_text);
        let existing = self.get_submissions(&round_id).await;
        for existing_sub in &existing {
            // Skip the submission being updated
            if existing_sub.id == submission_id {
                continue;
            }
            if normalize(&existing_sub.original_text) == normalized_new {
                return Err("DUPLICATE_EXACT".to_string());
            }
        }

        // Update the submission
        {
            let mut submissions = self.submissions.write().await;
            if let Some(submission) = submissions.get_mut(submission_id) {
                submission.original_text = new_text.clone();
                submission.display_text = new_text;
            }
        }

        // Broadcast updated submissions list
        self.broadcast_submissions(&round_id).await;

        Ok(())
    }
}
