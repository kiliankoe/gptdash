use super::AppState;
use crate::types::*;

impl AppState {
    /// Submit an answer
    pub async fn submit_answer(
        &self,
        round_id: &str,
        player_id: Option<String>,
        text: String,
    ) -> Result<Submission, String> {
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
    async fn broadcast_submissions(&self, round_id: &str) {
        let submissions = self.get_submissions(round_id).await;

        // Public broadcast (no author_kind to prevent spoilers)
        let public_infos: Vec<_> = submissions.iter().map(|s| s.into()).collect();
        self.broadcast_to_all(crate::protocol::ServerMessage::Submissions {
            list: public_infos,
        });

        // Host-only broadcast (includes author_kind for managing the game)
        let host_infos: Vec<_> = submissions.iter().map(|s| s.into()).collect();
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
}
