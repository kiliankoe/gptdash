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
    pub choices: Vec<String>,
    pub correct_index: usize,
    pub vote_counts: Vec<u32>,
    pub total_votes: u32,
}

const MAX_TRIVIA_QUESTION_LENGTH: usize = 500;
const MAX_TRIVIA_CHOICE_LENGTH: usize = 200;

impl AppState {
    // =========================================================================
    // Trivia Question CRUD
    // =========================================================================

    /// Add a new trivia question to the pool (2-4 choices)
    pub async fn add_trivia_question(
        &self,
        question: String,
        choices: Vec<TriviaChoiceInput>,
    ) -> Result<TriviaQuestion, String> {
        // Validate question length
        if question.len() > MAX_TRIVIA_QUESTION_LENGTH {
            return Err(format!(
                "Question too long ({} chars, max {})",
                question.len(),
                MAX_TRIVIA_QUESTION_LENGTH
            ));
        }

        // Validate number of choices
        if choices.len() < 2 || choices.len() > 4 {
            return Err("Trivia question must have 2-4 choices".to_string());
        }

        // Validate choice lengths
        for (i, choice) in choices.iter().enumerate() {
            if choice.text.len() > MAX_TRIVIA_CHOICE_LENGTH {
                return Err(format!(
                    "Choice {} too long ({} chars, max {})",
                    i + 1,
                    choice.text.len(),
                    MAX_TRIVIA_CHOICE_LENGTH
                ));
            }
        }

        // Validate exactly one correct answer
        let correct_count = choices.iter().filter(|c| c.is_correct).count();
        if correct_count != 1 {
            return Err("Trivia question must have exactly one correct answer".to_string());
        }

        let now = chrono::Utc::now().to_rfc3339();

        let trivia_choices: Vec<TriviaChoice> = choices
            .into_iter()
            .map(|c| TriviaChoice {
                text: c.text,
                is_correct: c.is_correct,
            })
            .collect();

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
        Ok(trivia_question)
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
        let vote_counts = self
            .get_trivia_vote_counts(&active_id, question.choices.len())
            .await;
        let total_votes: u32 = vote_counts.iter().sum();

        // Extract choice texts
        let choices: Vec<String> = question.choices.iter().map(|c| c.text.clone()).collect();

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
        // Get active trivia question ID
        let question_id = self
            .active_trivia
            .read()
            .await
            .clone()
            .ok_or_else(|| "No active trivia question".to_string())?;

        // Get the question to validate choice_index against actual number of choices
        let question = self
            .get_trivia_question(&question_id)
            .await
            .ok_or_else(|| "Trivia question not found".to_string())?;

        // Validate choice index against actual number of choices
        if choice_index >= question.choices.len() {
            return Err(format!(
                "Invalid choice index (must be 0-{})",
                question.choices.len() - 1
            ));
        }

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

    /// Get vote counts for a trivia question as a Vec (one count per choice)
    pub async fn get_trivia_vote_counts(
        &self,
        question_id: &TriviaQuestionId,
        num_choices: usize,
    ) -> Vec<u32> {
        let votes = self.trivia_votes.read().await;
        let question_votes = match votes.get(question_id) {
            Some(v) => v,
            None => return vec![0; num_choices],
        };

        let mut counts = vec![0u32; num_choices];
        for vote in question_votes {
            if vote.choice_index < num_choices {
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

        // Get the question to know how many choices it has
        let question = match self.get_trivia_question(&active_id).await {
            Some(q) => q,
            None => return 0,
        };

        let counts = self
            .get_trivia_vote_counts(&active_id, question.choices.len())
            .await;
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

    fn make_choices(correct_index: usize) -> Vec<TriviaChoiceInput> {
        vec![
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

    fn make_choices_2(correct_index: usize) -> Vec<TriviaChoiceInput> {
        vec![
            TriviaChoiceInput {
                text: "Choice A".to_string(),
                is_correct: correct_index == 0,
            },
            TriviaChoiceInput {
                text: "Choice B".to_string(),
                is_correct: correct_index == 1,
            },
        ]
    }

    fn make_choices_4(correct_index: usize) -> Vec<TriviaChoiceInput> {
        vec![
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
            TriviaChoiceInput {
                text: "Choice D".to_string(),
                is_correct: correct_index == 3,
            },
        ]
    }

    #[tokio::test]
    async fn test_add_trivia_question() {
        let state = AppState::new();

        let question = state
            .add_trivia_question("What is 2+2?".to_string(), make_choices(1))
            .await
            .unwrap();

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
            .await
            .unwrap();
        let _q2 = state
            .add_trivia_question("Q2".to_string(), make_choices(1))
            .await
            .unwrap();

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
            .await
            .unwrap();

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();

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
            .await
            .unwrap();
        let q2 = state
            .add_trivia_question("Q2?".to_string(), make_choices(1))
            .await
            .unwrap();

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();

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
            .await
            .unwrap();

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
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

        // Check counts (3 choices)
        let counts = state.get_trivia_vote_counts(&question.id, 3).await;
        assert_eq!(counts, vec![1, 2, 0]);
    }

    #[tokio::test]
    async fn test_trivia_vote_replacement() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(0))
            .await
            .unwrap();

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
        state.present_trivia(&question.id).await.unwrap();

        // Initial vote for choice 0
        state
            .submit_trivia_vote(&"voter1".to_string(), 0)
            .await
            .unwrap();
        assert_eq!(
            state.get_trivia_vote_counts(&question.id, 3).await,
            vec![1, 0, 0]
        );

        // Change vote to choice 2
        state
            .submit_trivia_vote(&"voter1".to_string(), 2)
            .await
            .unwrap();
        assert_eq!(
            state.get_trivia_vote_counts(&question.id, 3).await,
            vec![0, 0, 1]
        );
    }

    #[tokio::test]
    async fn test_trivia_resolve() {
        let state = AppState::new();
        state.create_game().await;

        // Correct answer is choice 1 (B)
        let question = state
            .add_trivia_question("Test?".to_string(), make_choices(1))
            .await
            .unwrap();

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
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
        assert_eq!(result.vote_counts, vec![1, 2, 0]);
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
            .await
            .unwrap();

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
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
            .await
            .unwrap();

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
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
            .await
            .unwrap();

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
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

    #[tokio::test]
    async fn test_two_choice_question() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("True or False?".to_string(), make_choices_2(1))
            .await
            .unwrap();

        // Verify 2 choices stored
        assert_eq!(question.choices.len(), 2);
        assert_eq!(question.choices[0].text, "Choice A");
        assert_eq!(question.choices[1].text, "Choice B");
        assert!(question.choices[1].is_correct);

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
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

        // Check counts (2 choices)
        let counts = state.get_trivia_vote_counts(&question.id, 2).await;
        assert_eq!(counts, vec![1, 1]);

        // Trying to vote for choice 2 should fail
        let result = state.submit_trivia_vote(&"voter3".to_string(), 2).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid choice index"));
    }

    #[tokio::test]
    async fn test_four_choice_question() {
        let state = AppState::new();
        state.create_game().await;

        let question = state
            .add_trivia_question("Pick a letter?".to_string(), make_choices_4(3))
            .await
            .unwrap();

        // Verify 4 choices stored
        assert_eq!(question.choices.len(), 4);
        assert_eq!(question.choices[3].text, "Choice D");
        assert!(question.choices[3].is_correct);

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
        state
            .transition_phase(GamePhase::Writing, None)
            .await
            .unwrap();
        state.present_trivia(&question.id).await.unwrap();

        // Vote for all 4 choices
        state
            .submit_trivia_vote(&"voter1".to_string(), 0)
            .await
            .unwrap();
        state
            .submit_trivia_vote(&"voter2".to_string(), 1)
            .await
            .unwrap();
        state
            .submit_trivia_vote(&"voter3".to_string(), 2)
            .await
            .unwrap();
        state
            .submit_trivia_vote(&"voter4".to_string(), 3)
            .await
            .unwrap();

        // Check counts (4 choices)
        let counts = state.get_trivia_vote_counts(&question.id, 4).await;
        assert_eq!(counts, vec![1, 1, 1, 1]);

        // Resolve and verify
        let result = state.resolve_trivia().await.unwrap();
        assert_eq!(result.choices.len(), 4);
        assert_eq!(result.vote_counts.len(), 4);
        assert_eq!(result.correct_index, 3);
        assert_eq!(result.total_votes, 4);
    }
}
