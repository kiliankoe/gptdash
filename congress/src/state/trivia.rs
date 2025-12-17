//! Trivia system state management
//!
//! Provides audience entertainment during the WRITING phase.
//! Host manages trivia questions, can present one at a time, and resolve to show results.

use crate::types::{TriviaChoice, TriviaQuestion, TriviaQuestionId, TriviaVote, VoterId};

use super::AppState;

/// Input for creating a new trivia question choice
#[derive(Debug, Clone)]
pub struct TriviaChoiceInput {
    pub text: String,
    pub is_correct: bool,
}

/// Result data returned when resolving a trivia question
#[derive(Debug, Clone)]
pub struct TriviaResultData {
    pub question_id: TriviaQuestionId,
    pub question: String,
    pub choices: [String; 3],
    pub correct_index: usize,
    pub vote_counts: [u32; 3],
    pub total_votes: u32,
}

impl AppState {
    // =========================================================================
    // Trivia Question CRUD
    // =========================================================================

    /// Add a new trivia question to the pool
    pub async fn add_trivia_question(
        &self,
        question: String,
        choices: [TriviaChoiceInput; 3],
    ) -> TriviaQuestion {
        let now = chrono::Utc::now().to_rfc3339();

        let trivia_choices = [
            TriviaChoice {
                text: choices[0].text.clone(),
                is_correct: choices[0].is_correct,
            },
            TriviaChoice {
                text: choices[1].text.clone(),
                is_correct: choices[1].is_correct,
            },
            TriviaChoice {
                text: choices[2].text.clone(),
                is_correct: choices[2].is_correct,
            },
        ];

        let trivia_question = TriviaQuestion {
            id: ulid::Ulid::new().to_string(),
            question,
            choices: trivia_choices,
            created_at: now,
        };

        self.trivia_questions
            .write()
            .await
            .push(trivia_question.clone());

        tracing::info!("Added trivia question: {}", trivia_question.id);
        trivia_question
    }

    /// Remove a trivia question from the pool by ID
    /// Returns true if the question was found and removed
    pub async fn remove_trivia_question(&self, id: &TriviaQuestionId) -> bool {
        let mut questions = self.trivia_questions.write().await;
        if let Some(pos) = questions.iter().position(|q| &q.id == id) {
            questions.remove(pos);
            tracing::info!("Removed trivia question: {}", id);

            // Also clear any votes for this question
            self.trivia_votes.write().await.remove(id);

            // Clear active trivia if it was this question
            let mut active = self.active_trivia.write().await;
            if active.as_ref() == Some(id) {
                *active = None;
            }

            true
        } else {
            false
        }
    }

    /// Get all trivia questions
    pub async fn get_trivia_questions(&self) -> Vec<TriviaQuestion> {
        self.trivia_questions.read().await.clone()
    }

    /// Get a trivia question by ID
    pub async fn get_trivia_question(&self, id: &TriviaQuestionId) -> Option<TriviaQuestion> {
        self.trivia_questions
            .read()
            .await
            .iter()
            .find(|q| &q.id == id)
            .cloned()
    }

    // =========================================================================
    // Trivia Presentation Flow
    // =========================================================================

    /// Present a trivia question to audience (only during WRITING phase)
    /// Sets the question as active and clears previous votes for it
    pub async fn present_trivia(&self, id: &TriviaQuestionId) -> Result<TriviaQuestion, String> {
        // Verify question exists
        let question = self
            .get_trivia_question(id)
            .await
            .ok_or_else(|| "Trivia question not found".to_string())?;

        // Verify we're in WRITING phase
        let game = self.game.read().await;
        if let Some(ref game) = *game {
            if game.phase != crate::types::GamePhase::Writing {
                return Err("Trivia can only be presented during WRITING phase".to_string());
            }
        } else {
            return Err("No game in progress".to_string());
        }
        drop(game);

        // Check if there's already an active trivia
        let active = self.active_trivia.read().await;
        if active.is_some() {
            return Err("Another trivia question is already active".to_string());
        }
        drop(active);

        // Clear any previous votes for this question (fresh start)
        self.trivia_votes.write().await.remove(id);

        // Set as active
        *self.active_trivia.write().await = Some(id.clone());

        tracing::info!("Presented trivia question: {}", id);
        Ok(question)
    }

    /// Get the currently active trivia question (if any)
    pub async fn get_active_trivia(&self) -> Option<TriviaQuestion> {
        let active_id = self.active_trivia.read().await.clone()?;
        self.get_trivia_question(&active_id).await
    }

    /// Get the currently active trivia question ID (if any)
    pub async fn get_active_trivia_id(&self) -> Option<TriviaQuestionId> {
        self.active_trivia.read().await.clone()
    }

    /// Resolve the current trivia question - compute results and clear active state
    /// Returns the result data for display
    pub async fn resolve_trivia(&self) -> Option<TriviaResultData> {
        // Get active question
        let active_id = self.active_trivia.read().await.clone()?;
        let question = self.get_trivia_question(&active_id).await?;

        // Find correct answer index
        let correct_index = question
            .choices
            .iter()
            .position(|c| c.is_correct)
            .unwrap_or(0);

        // Get vote counts
        let vote_counts = self.get_trivia_vote_counts(&active_id).await;
        let total_votes: u32 = vote_counts.iter().sum();

        // Extract choice texts
        let choices = [
            question.choices[0].text.clone(),
            question.choices[1].text.clone(),
            question.choices[2].text.clone(),
        ];

        // Clear active trivia (but keep votes for potential re-display)
        *self.active_trivia.write().await = None;

        tracing::info!(
            "Resolved trivia question: {} (correct: {}, votes: {})",
            active_id,
            correct_index,
            total_votes
        );

        Some(TriviaResultData {
            question_id: active_id,
            question: question.question,
            choices,
            correct_index,
            vote_counts,
            total_votes,
        })
    }

    /// Clear the active trivia without showing results
    pub async fn clear_trivia(&self) {
        let active_id = self.active_trivia.write().await.take();
        if let Some(id) = active_id {
            tracing::info!("Cleared trivia question without resolving: {}", id);
        }
    }

    /// Clear all trivia questions and votes (for game reset)
    pub async fn clear_trivia_questions(&self) {
        self.trivia_questions.write().await.clear();
        self.active_trivia.write().await.take();
        self.trivia_votes.write().await.clear();
        tracing::info!("Trivia questions cleared");
    }

    // =========================================================================
    // Trivia Voting
    // =========================================================================

    /// Submit a trivia vote (one vote per audience member per question)
    /// Replaces previous vote if the voter has already voted
    pub async fn submit_trivia_vote(
        &self,
        voter_id: &VoterId,
        choice_index: usize,
    ) -> Result<(), String> {
        // Validate choice index (must be 0, 1, or 2)
        if choice_index > 2 {
            return Err("Invalid choice index (must be 0, 1, or 2)".to_string());
        }

        // Get active trivia question ID
        let question_id = self
            .active_trivia
            .read()
            .await
            .clone()
            .ok_or_else(|| "No active trivia question".to_string())?;

        // Create the vote
        let vote = TriviaVote {
            voter_id: voter_id.clone(),
            question_id: question_id.clone(),
            choice_index,
        };

        // Store/replace vote
        let mut votes = self.trivia_votes.write().await;
        let question_votes = votes.entry(question_id.clone()).or_default();

        // Remove previous vote from this voter if exists
        question_votes.retain(|v| v.voter_id != *voter_id);

        // Add new vote
        question_votes.push(vote);

        tracing::info!(
            "Trivia vote recorded: voter={}, question={}, choice={}",
            voter_id,
            question_id,
            choice_index
        );

        Ok(())
    }

    /// Get vote counts for a trivia question [count_for_0, count_for_1, count_for_2]
    pub async fn get_trivia_vote_counts(&self, question_id: &TriviaQuestionId) -> [u32; 3] {
        let votes = self.trivia_votes.read().await;
        let question_votes = match votes.get(question_id) {
            Some(v) => v,
            None => return [0, 0, 0],
        };

        let mut counts = [0u32; 3];
        for vote in question_votes {
            if vote.choice_index < 3 {
                counts[vote.choice_index] += 1;
            }
        }

        counts
    }

    /// Get the total number of votes for the currently active trivia question
    pub async fn get_active_trivia_vote_count(&self) -> u32 {
        let active_id = match self.active_trivia.read().await.clone() {
            Some(id) => id,
            None => return 0,
        };

        let counts = self.get_trivia_vote_counts(&active_id).await;
        counts.iter().sum()
    }

    /// Get a voter's current trivia vote for the active question (if any)
    pub async fn get_trivia_vote(&self, voter_id: &VoterId) -> Option<TriviaVote> {
        let question_id = self.active_trivia.read().await.clone()?;
        let votes = self.trivia_votes.read().await;
        let question_votes = votes.get(&question_id)?;

        question_votes
            .iter()
            .find(|v| &v.voter_id == voter_id)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GamePhase;

    fn make_choices(correct_index: usize) -> [TriviaChoiceInput; 3] {
        [
            TriviaChoiceInput {
                text: "Choice A".to_string(),
                is_correct: correct_index == 0,
            },
            TriviaChoiceInput {
                text: "Choice B".to_string(),
                is_correct: correct_index == 1,
            },
            TriviaChoiceInput {
                text: "Choice C".to_string(),
                is_correct: correct_index == 2,
            },
        ]
    }

    #[tokio::test]
    async fn test_add_trivia_question() {
        let state = AppState::new();

        let question = state
            .add_trivia_question("What is 2+2?".to_string(), make_choices(1))
            .await;

        assert_eq!(question.question, "What is 2+2?");
        assert!(!question.id.is_empty());
        assert_eq!(question.choices[1].is_correct, true);
        assert_eq!(question.choices[0].is_correct, false);

        // Verify it's in the pool
        let questions = state.get_trivia_questions().await;
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].id, question.id);
    }

    #[tokio::test]
    async fn test_remove_trivia_question() {
        let state = AppState::new();

        let q1 = state
            .add_trivia_question("Q1".to_string(), make_choices(0))
            .await;
        let _q2 = state
            .add_trivia_question("Q2".to_string(), make_choices(1))
            .await;

        assert_eq!(state.get_trivia_questions().await.len(), 2);

        // Remove q1
        let removed = state.remove_trivia_question(&q1.id).await;
        assert!(removed);

        // Only q2 remains
        let questions = state.get_trivia_questions().await;
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question, "Q2");

        // Removing again returns false
        let removed_again = state.remove_trivia_question(&q1.id).await;
        assert!(!removed_again);
    }

    #[tokio::test]
    async fn test_present_trivia_requires_writing_phase() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(0))
            .await;

        // In LOBBY phase - should fail
        let result = state.present_trivia(&question.id).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("only be presented during WRITING"));

        // Transition to WRITING phase (need to set up round with prompt first)
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test prompt".to_string()),
                None,
                crate::types::PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();

        // Now should succeed
        let result = state.present_trivia(&question.id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cannot_present_two_trivia_at_once() {
        let state = AppState::new();
        state.create_game().await;

        let q1 = state
            .add_trivia_question("Q1?".to_string(), make_choices(0))
            .await;
        let q2 = state
            .add_trivia_question("Q2?".to_string(), make_choices(1))
            .await;

        // Set up WRITING phase
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test".to_string()),
                None,
                crate::types::PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();

        // Present q1
        state.present_trivia(&q1.id).await.unwrap();

        // Trying to present q2 should fail
        let result = state.present_trivia(&q2.id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already active"));
    }

    #[tokio::test]
    async fn test_trivia_voting() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(1))
            .await;

        // Set up WRITING phase and present trivia
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test".to_string()),
                None,
                crate::types::PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();
        state.present_trivia(&question.id).await.unwrap();

        // Vote
        state
            .submit_trivia_vote(&"voter1".to_string(), 0)
            .await
            .unwrap();
        state
            .submit_trivia_vote(&"voter2".to_string(), 1)
            .await
            .unwrap();
        state
            .submit_trivia_vote(&"voter3".to_string(), 1)
            .await
            .unwrap();

        // Check counts
        let counts = state.get_trivia_vote_counts(&question.id).await;
        assert_eq!(counts, [1, 2, 0]);
    }

    #[tokio::test]
    async fn test_trivia_vote_replacement() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(0))
            .await;

        // Set up WRITING phase and present trivia
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test".to_string()),
                None,
                crate::types::PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();
        state.present_trivia(&question.id).await.unwrap();

        // Initial vote for choice 0
        state
            .submit_trivia_vote(&"voter1".to_string(), 0)
            .await
            .unwrap();
        assert_eq!(state.get_trivia_vote_counts(&question.id).await, [1, 0, 0]);

        // Change vote to choice 2
        state
            .submit_trivia_vote(&"voter1".to_string(), 2)
            .await
            .unwrap();
        assert_eq!(state.get_trivia_vote_counts(&question.id).await, [0, 0, 1]);
    }

    #[tokio::test]
    async fn test_trivia_resolve() {
        let state = AppState::new();
        state.create_game().await;

        // Correct answer is choice 1 (B)
        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(1))
            .await;

        // Set up WRITING phase and present trivia
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test".to_string()),
                None,
                crate::types::PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();
        state.present_trivia(&question.id).await.unwrap();

        // Add votes
        state
            .submit_trivia_vote(&"voter1".to_string(), 0)
            .await
            .unwrap();
        state
            .submit_trivia_vote(&"voter2".to_string(), 1)
            .await
            .unwrap();
        state
            .submit_trivia_vote(&"voter3".to_string(), 1)
            .await
            .unwrap();

        // Resolve
        let result = state.resolve_trivia().await.unwrap();

        assert_eq!(result.question_id, question.id);
        assert_eq!(result.question, "Test?");
        assert_eq!(result.correct_index, 1);
        assert_eq!(result.vote_counts, [1, 2, 0]);
        assert_eq!(result.total_votes, 3);

        // Active trivia should be cleared
        assert!(state.get_active_trivia().await.is_none());
    }

    #[tokio::test]
    async fn test_trivia_clear() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(0))
            .await;

        // Set up WRITING phase and present trivia
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test".to_string()),
                None,
                crate::types::PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();
        state.present_trivia(&question.id).await.unwrap();

        // Verify active
        assert!(state.get_active_trivia().await.is_some());

        // Clear
        state.clear_trivia().await;

        // Active should be None
        assert!(state.get_active_trivia().await.is_none());
    }

    #[tokio::test]
    async fn test_invalid_vote_choice_index() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(0))
            .await;

        // Set up WRITING phase and present trivia
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test".to_string()),
                None,
                crate::types::PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();
        state.present_trivia(&question.id).await.unwrap();

        // Invalid choice index (3 is out of bounds)
        let result = state.submit_trivia_vote(&"voter1".to_string(), 3).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid choice index"));
    }

    #[tokio::test]
    async fn test_vote_without_active_trivia() {
        let state = AppState::new();
        state.create_game().await;

        // No trivia presented
        let result = state.submit_trivia_vote(&"voter1".to_string(), 0).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No active trivia"));
    }

    #[tokio::test]
    async fn test_get_trivia_vote() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(0))
            .await;

        // Set up WRITING phase and present trivia
        let round = state.start_round().await.unwrap();
        let prompt = state
            .add_prompt_to_pool(
                Some("Test".to_string()),
                None,
                crate::types::PromptSource::Host,
                None,
            )
            .await
            .unwrap();
        state
            .select_prompt(&round.id, &prompt.id, None)
            .await
            .unwrap();
        state.transition_phase(GamePhase::Writing).await.unwrap();
        state.present_trivia(&question.id).await.unwrap();

        // No vote yet
        assert!(state.get_trivia_vote(&"voter1".to_string()).await.is_none());

        // Submit vote
        state
            .submit_trivia_vote(&"voter1".to_string(), 2)
            .await
            .unwrap();

        // Now should find it
        let vote = state.get_trivia_vote(&"voter1".to_string()).await.unwrap();
        assert_eq!(vote.choice_index, 2);
        assert_eq!(vote.voter_id, "voter1");
    }
}
