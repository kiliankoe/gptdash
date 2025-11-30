use super::AppState;
use crate::types::*;

/// Normalize text for duplicate comparison (trim whitespace, lowercase)
fn normalize(text: &str) -> String {
    text.trim().to_lowercase()
}

impl AppState {
    /// Submit an answer with automatic exact duplicate detection
    pub async fn submit_answer(
        &self,
        round_id: &str,
        player_id: Option<String>,
        text: String,
    ) -> Result<Submission, String> {
        // Check for exact duplicates
        let normalized_new = normalize(&text);
        let existing = self.get_submissions(round_id).await;
        for existing_sub in &existing {
            if normalize(&existing_sub.original_text) == normalized_new {
                return Err("DUPLICATE_EXACT".to_string());
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

    /// Broadcast submissions list for a round to all clients
    pub async fn broadcast_submissions(&self, round_id: &str) {
        let submissions = self.get_submissions(round_id).await;
        tracing::info!(
            "Broadcasting {} submissions for round {}",
            submissions.len(),
            round_id
        );

        // Public broadcast (no author_kind to prevent spoilers)
        let public_infos: Vec<_> = submissions.iter().map(|s| s.into()).collect();
        self.broadcast_to_all(crate::protocol::ServerMessage::Submissions { list: public_infos });

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

        // Broadcast updated submissions list
        self.broadcast_submissions(&round_id).await;

        Ok(player_id)
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
