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
        Ok(submission)
    }

    /// Edit a submission (host only)
    pub async fn edit_submission(
        &self,
        submission_id: &str,
        new_text: String,
    ) -> Result<(), String> {
        let mut submissions = self.submissions.write().await;
        if let Some(submission) = submissions.get_mut(submission_id) {
            submission.display_text = new_text;
            submission.edited_by_host = Some(true);
            Ok(())
        } else {
            Err("Submission not found".to_string())
        }
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
