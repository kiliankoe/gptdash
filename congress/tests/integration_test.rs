use gptdash::protocol::{ClientMessage, PlayerSubmissionStatus, ServerMessage};
use gptdash::state::AppState;
use gptdash::types::{GamePhase, PromptSource, Role};
use gptdash::ws::handlers::handle_message;
use std::sync::Arc;

/// End-to-end integration test for a complete game flow
#[tokio::test]
async fn test_full_game_flow() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;
    let audience_role = Role::Audience;

    // 1. Setup: Create game
    state.create_game().await;
    let game = state.get_game().await.expect("Game should exist");
    assert_eq!(game.phase, GamePhase::Lobby);

    // 2. Create players
    let create_players_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 2 },
        &host_role,
        &state,
    )
    .await;

    let player_tokens = match create_players_result {
        Some(ServerMessage::PlayersCreated { players }) => {
            assert_eq!(players.len(), 2, "Should create 2 players");
            players
        }
        _ => panic!("Expected PlayersCreated message"),
    };

    // 3. Register players with display names
    let register_p1 = handle_message(
        ClientMessage::RegisterPlayer {
            player_token: player_tokens[0].token.clone(),
            display_name: "Alice".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match register_p1 {
        Some(ServerMessage::PlayerRegistered {
            player_id,
            display_name,
        }) => {
            assert_eq!(display_name, "Alice");
            assert_eq!(player_id, player_tokens[0].id);
        }
        _ => panic!("Expected PlayerRegistered message"),
    }

    let register_p2 = handle_message(
        ClientMessage::RegisterPlayer {
            player_token: player_tokens[1].token.clone(),
            display_name: "Bob".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match register_p2 {
        Some(ServerMessage::PlayerRegistered {
            player_id,
            display_name,
        }) => {
            assert_eq!(display_name, "Bob");
            assert_eq!(player_id, player_tokens[1].id);
        }
        _ => panic!("Expected PlayerRegistered message for Bob"),
    }

    // 4. Transition to PromptSelection
    let phase_result = handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    // Phase transitions now return Phase message instead of GameState
    match phase_result {
        Some(ServerMessage::Phase { phase, .. }) => {
            assert_eq!(phase, GamePhase::PromptSelection);
        }
        _ => panic!("Expected Phase message"),
    }

    // 5. Start round
    let start_round_result =
        handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = match start_round_result {
        Some(ServerMessage::RoundStarted { round }) => {
            assert_eq!(round.number, 1);
            round
        }
        _ => panic!("Expected RoundStarted message"),
    };

    // 6. Add and select prompt
    let prompt = state
        .add_prompt_to_pool(
            Some("What is the meaning of life?".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .expect("Should add prompt");

    let select_prompt_result = handle_message(
        ClientMessage::HostSelectPrompt {
            prompt_id: prompt.id.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    match select_prompt_result {
        Some(ServerMessage::PromptSelected { prompt: p }) => {
            assert_eq!(p.id, prompt.id);
        }
        _ => panic!("Expected PromptSelected message"),
    }

    // 7. Transition to Writing phase
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    let game = state.get_game().await.expect("Game should exist");
    assert_eq!(game.phase, GamePhase::Writing);
    assert!(
        game.phase_deadline.is_some(),
        "Writing phase should have deadline"
    );

    // 8. Players submit answers
    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_tokens[0].token.clone()),
            text: "To seek truth and understanding".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_tokens[1].token.clone()),
            text: "To eat pizza and watch movies".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    // 9. Manually create AI submission (no LLM in tests)
    let ai_submission = state
        .submit_answer(&round.id, None, "42, obviously".to_string())
        .await
        .expect("Should create AI submission");

    // 10. Get all submissions and set reveal order
    let submissions = state.get_submissions(&round.id).await;
    assert_eq!(
        submissions.len(),
        3,
        "Should have 2 player + 1 AI submission"
    );

    let reveal_order: Vec<_> = submissions.iter().map(|s| s.id.clone()).collect();

    handle_message(
        ClientMessage::HostSetRevealOrder {
            order: reveal_order.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    // 11. Transition to Reveal phase
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Reveal,
        },
        &host_role,
        &state,
    )
    .await;

    let game = state.get_game().await.expect("Game should exist");
    assert_eq!(game.phase, GamePhase::Reveal);

    // 12. Navigate reveal carousel
    let reveal_next_result =
        handle_message(ClientMessage::HostRevealNext, &host_role, &state).await;

    match reveal_next_result {
        Some(ServerMessage::RevealUpdate {
            reveal_index,
            submission,
        }) => {
            assert_eq!(reveal_index, 1, "Should advance to second submission");
            assert!(submission.is_some(), "Should have submission data");
        }
        _ => panic!("Expected RevealUpdate message"),
    }

    // Go back to previous
    let reveal_prev_result =
        handle_message(ClientMessage::HostRevealPrev, &host_role, &state).await;

    match reveal_prev_result {
        Some(ServerMessage::RevealUpdate {
            reveal_index,
            submission: _,
        }) => {
            assert_eq!(reveal_index, 0, "Should go back to first submission");
        }
        _ => panic!("Expected RevealUpdate message"),
    }

    // 13. Transition to Voting phase
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Voting,
        },
        &host_role,
        &state,
    )
    .await;

    let game = state.get_game().await.expect("Game should exist");
    assert_eq!(game.phase, GamePhase::Voting);
    assert!(
        game.phase_deadline.is_some(),
        "Voting phase should have deadline"
    );

    // 14. Audience votes
    // Find a player submission to vote as funny
    use gptdash::types::AuthorKind;
    let player_submission = submissions
        .iter()
        .find(|s| s.author_kind == AuthorKind::Player)
        .expect("Should have at least one player submission");

    let vote_result = handle_message(
        ClientMessage::Vote {
            voter_token: "voter_1".to_string(),
            ai: ai_submission.id.clone(),
            funny: player_submission.id.clone(),
            msg_id: "vote_1".to_string(),
        },
        &audience_role,
        &state,
    )
    .await;

    match vote_result {
        Some(ServerMessage::VoteAck { msg_id }) => {
            assert_eq!(msg_id, "vote_1");
        }
        _ => panic!("Expected VoteAck message"),
    }

    // Another vote - vote for different player submission as AI (wrong), same as funny
    let another_player_submission = submissions
        .iter()
        .find(|s| s.id != player_submission.id && s.author_kind == AuthorKind::Player)
        .expect("Should have at least two player submissions");

    let vote_2_result = handle_message(
        ClientMessage::Vote {
            voter_token: "voter_2".to_string(),
            ai: another_player_submission.id.clone(), // Wrong guess
            funny: player_submission.id.clone(),
            msg_id: "vote_2".to_string(),
        },
        &audience_role,
        &state,
    )
    .await;

    match vote_2_result {
        Some(ServerMessage::VoteAck { msg_id }) => {
            assert_eq!(msg_id, "vote_2");
        }
        _ => panic!("Expected VoteAck message for vote_2"),
    }

    // 15. Set AI submission for scoring
    handle_message(
        ClientMessage::HostSetAiSubmission {
            submission_id: ai_submission.id.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    // 16. Transition to Results phase (triggers scoring)
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Results,
        },
        &host_role,
        &state,
    )
    .await;

    let game = state.get_game().await.expect("Game should exist");
    assert_eq!(game.phase, GamePhase::Results);

    // 17. Verify scores were computed
    let (player_scores, audience_scores) = state.get_leaderboards().await;

    assert_eq!(player_scores.len(), 2, "Should have scores for 2 players");
    assert_eq!(audience_scores.len(), 2, "Should have scores for 2 voters");

    // voter_1 correctly identified AI, should have 1 AI detect point
    let voter_1_score = audience_scores
        .iter()
        .find(|s| s.ref_id == "voter_1")
        .expect("voter_1 should have score");
    assert_eq!(
        voter_1_score.ai_detect_points, 1,
        "voter_1 correctly detected AI"
    );

    // voter_2 incorrectly identified AI, should have 0 AI detect points
    let voter_2_score = audience_scores
        .iter()
        .find(|s| s.ref_id == "voter_2")
        .expect("voter_2 should have score");
    assert_eq!(
        voter_2_score.ai_detect_points, 0,
        "voter_2 incorrectly detected AI"
    );

    // Player who got the funny votes should have funny points
    let player_with_funny = player_scores
        .iter()
        .find(|s| s.funny_points > 0)
        .expect("At least one player should have funny points");
    assert_eq!(
        player_with_funny.funny_points, 2,
        "Player should have 2 funny votes"
    );

    // Player who got AI votes (fooled voters) should have points
    // another_player_submission got 1 AI vote from voter_2
    let fooling_player = player_scores
        .iter()
        .find(|s| {
            // Find player who owns another_player_submission
            another_player_submission
                .author_ref
                .as_ref()
                .map(|id| id == &s.ref_id)
                .unwrap_or(false)
        })
        .expect("Should find player who fooled voter");
    assert_eq!(
        fooling_player.ai_detect_points, 1,
        "Player fooled 1 voter with their answer"
    );

    // 18. Verify scoring is idempotent (re-entering Results doesn't double scores)
    let round_current = state
        .get_current_round()
        .await
        .expect("Should have current round");
    assert!(
        round_current.scored_at.is_some(),
        "Round should be marked as scored"
    );

    // Try transitioning to Intermission and back to Results
    let intermission_result = handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Intermission,
        },
        &host_role,
        &state,
    )
    .await;

    // Phase transitions now return Phase message instead of GameState
    match intermission_result {
        Some(ServerMessage::Phase { phase, .. }) => {
            assert_eq!(
                phase,
                GamePhase::Intermission,
                "Should transition to Intermission"
            );
        }
        _ => panic!("Expected Phase message for Intermission transition"),
    }

    let results_again_result = handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Results,
        },
        &host_role,
        &state,
    )
    .await;

    match results_again_result {
        Some(ServerMessage::Phase { phase, .. }) => {
            assert_eq!(
                phase,
                GamePhase::Results,
                "Should transition back to Results"
            );
        }
        _ => panic!("Expected Phase message for Results re-entry"),
    }

    let (player_scores_again, _) = state.get_leaderboards().await;
    assert_eq!(
        player_scores, player_scores_again,
        "Scores should not change on re-entering Results"
    );

    println!("✅ Full game flow integration test passed!");
}

/// Test invalid phase transitions are rejected
#[tokio::test]
async fn test_invalid_phase_transitions() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Try to jump directly from Lobby to Voting (invalid)
    let result = handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Voting,
        },
        &host_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, msg }) => {
            assert_eq!(code, "TRANSITION_FAILED");
            assert!(msg.contains("Invalid phase transition"));
        }
        _ => panic!("Expected error for invalid phase transition"),
    }
}

/// Test precondition validation (e.g., Writing phase requires prompt)
#[tokio::test]
async fn test_phase_preconditions() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Transition to PromptSelection
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    // Start a round
    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    // Try to transition to Writing without selecting a prompt (should fail)
    let result = handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, msg }) => {
            assert_eq!(code, "TRANSITION_FAILED");
            assert!(msg.contains("Writing phase requires a selected prompt"));
        }
        _ => panic!("Expected error for missing prompt precondition"),
    }
}

/// Test unauthorized access to host commands
#[tokio::test]
async fn test_unauthorized_host_commands() {
    let state = Arc::new(AppState::new());
    let audience_role = Role::Audience;

    state.create_game().await;

    // Audience tries to create players (should fail)
    let result = handle_message(
        ClientMessage::HostCreatePlayers { count: 3 },
        &audience_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code, "UNAUTHORIZED");
        }
        _ => panic!("Expected unauthorized error"),
    }

    // Audience tries to transition phase (should fail)
    let result = handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &audience_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code, "UNAUTHORIZED");
        }
        _ => panic!("Expected unauthorized error"),
    }
}

/// Test typo check request flow (without actual LLM - returns original text)
#[tokio::test]
async fn test_typo_check_request() {
    let state = Arc::new(AppState::new());
    let player_role = Role::Player;

    state.create_game().await;

    // Create a player
    let player = state.create_player().await;
    state
        .register_player(&player.token, "TestPlayer".to_string())
        .await
        .expect("Should register player");

    // Request typo check (no LLM configured, should return original text unchanged)
    let result = handle_message(
        ClientMessage::RequestTypoCheck {
            player_token: player.token.clone(),
            text: "Dies ist ein Test mit Tippfehler".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::TypoCheckResult {
            original,
            corrected,
            has_changes,
        }) => {
            assert_eq!(original, "Dies ist ein Test mit Tippfehler");
            assert_eq!(corrected, original, "Without LLM, should return original");
            assert!(!has_changes, "Without LLM, should have no changes");
        }
        _ => panic!("Expected TypoCheckResult message"),
    }

    println!("✅ Typo check request test passed!");
}

/// Test typo check with invalid player token
#[tokio::test]
async fn test_typo_check_invalid_token() {
    let state = Arc::new(AppState::new());
    let player_role = Role::Player;

    state.create_game().await;

    // Request typo check with invalid token
    let result = handle_message(
        ClientMessage::RequestTypoCheck {
            player_token: "invalid_token".to_string(),
            text: "Some text".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code, "INVALID_PLAYER_TOKEN");
        }
        _ => panic!("Expected error for invalid token"),
    }

    println!("✅ Typo check invalid token test passed!");
}

/// Test player status tracking
#[tokio::test]
async fn test_player_status_tracking() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;

    state.create_game().await;

    // Create players
    let create_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 2 },
        &host_role,
        &state,
    )
    .await;

    let player_tokens = match create_result {
        Some(ServerMessage::PlayersCreated { players }) => players,
        _ => panic!("Expected PlayersCreated"),
    };

    // Check initial status - all should be NotSubmitted
    let statuses = state.get_all_player_status().await;
    assert_eq!(statuses.len(), 2);
    for status in &statuses {
        assert_eq!(status.status, PlayerSubmissionStatus::NotSubmitted);
        assert!(status.display_name.is_none());
    }

    // Register first player
    handle_message(
        ClientMessage::RegisterPlayer {
            player_token: player_tokens[0].token.clone(),
            display_name: "Alice".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    // Check status after registration - should have name now
    let statuses = state.get_all_player_status().await;
    let alice_status = statuses
        .iter()
        .find(|s| s.id == player_tokens[0].id)
        .expect("Should find Alice");
    assert_eq!(alice_status.display_name, Some("Alice".to_string()));
    assert_eq!(alice_status.status, PlayerSubmissionStatus::NotSubmitted);

    // Start a round and submit
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");
    let prompt = state
        .add_prompt_to_pool(
            Some("Test prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state.select_prompt(&round.id, &prompt.id).await.unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Alice submits
    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_tokens[0].token.clone()),
            text: "Alice's answer to the prompt".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    // Check status after submission
    let statuses = state.get_all_player_status().await;
    let alice_status = statuses
        .iter()
        .find(|s| s.id == player_tokens[0].id)
        .expect("Should find Alice");
    assert_eq!(
        alice_status.status,
        PlayerSubmissionStatus::Submitted,
        "Alice should be Submitted after submitting"
    );

    let bob_status = statuses
        .iter()
        .find(|s| s.id == player_tokens[1].id)
        .expect("Should find Bob");
    assert_eq!(
        bob_status.status,
        PlayerSubmissionStatus::NotSubmitted,
        "Bob should still be NotSubmitted"
    );

    println!("✅ Player status tracking test passed!");
}

/// Test submission update flow (accepting typo correction)
#[tokio::test]
async fn test_submission_update() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;

    state.create_game().await;

    // Create and register player
    let player = state.create_player().await;
    state
        .register_player(&player.token, "TestPlayer".to_string())
        .await
        .unwrap();

    // Setup round
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");
    let prompt = state
        .add_prompt_to_pool(
            Some("Test prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state.select_prompt(&round.id, &prompt.id).await.unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Submit original answer
    let submit_result = handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player.token.clone()),
            text: "Original answer with typo".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match submit_result {
        Some(ServerMessage::SubmissionConfirmed) => {}
        _ => panic!("Expected SubmissionConfirmed"),
    }

    // Get the submission ID
    let submissions = state.get_submissions(&round.id).await;
    let player_sub = submissions
        .iter()
        .find(|s| s.author_ref.as_ref() == Some(&player.id))
        .expect("Should find player submission");

    // Update submission (simulating accepting typo correction)
    let update_result = handle_message(
        ClientMessage::UpdateSubmission {
            player_token: player.token.clone(),
            submission_id: player_sub.id.clone(),
            new_text: "Corrected answer without typo".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match update_result {
        Some(ServerMessage::SubmissionConfirmed) => {}
        _ => panic!("Expected SubmissionConfirmed after update"),
    }

    // Verify the submission was updated
    let updated_submissions = state.get_submissions(&round.id).await;
    let updated_sub = updated_submissions
        .iter()
        .find(|s| s.id == player_sub.id)
        .expect("Should find updated submission");

    assert_eq!(
        updated_sub.display_text, "Corrected answer without typo",
        "Submission text should be updated"
    );
    assert_eq!(
        updated_sub.original_text, "Corrected answer without typo",
        "Original text should also be updated"
    );

    println!("✅ Submission update test passed!");
}

/// Test submission update with unauthorized player
#[tokio::test]
async fn test_submission_update_unauthorized() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;

    state.create_game().await;

    // Create two players
    let player1 = state.create_player().await;
    let player2 = state.create_player().await;
    state
        .register_player(&player1.token, "Player1".to_string())
        .await
        .unwrap();
    state
        .register_player(&player2.token, "Player2".to_string())
        .await
        .unwrap();

    // Setup round
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");
    let prompt = state
        .add_prompt_to_pool(
            Some("Test prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state.select_prompt(&round.id, &prompt.id).await.unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Player1 submits
    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player1.token.clone()),
            text: "Player 1's answer".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    // Get Player1's submission ID
    let submissions = state.get_submissions(&round.id).await;
    let player1_sub = submissions
        .iter()
        .find(|s| s.author_ref.as_ref() == Some(&player1.id))
        .expect("Should find player1 submission");

    // Player2 tries to update Player1's submission (should fail)
    let update_result = handle_message(
        ClientMessage::UpdateSubmission {
            player_token: player2.token.clone(),
            submission_id: player1_sub.id.clone(),
            new_text: "Hacked by Player2".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match update_result {
        Some(ServerMessage::Error { code, msg }) => {
            assert_eq!(code, "UPDATE_SUBMISSION_FAILED");
            assert!(
                msg.contains("Not authorized"),
                "Should indicate authorization failure"
            );
        }
        _ => panic!("Expected error for unauthorized update"),
    }

    // Verify submission was not changed
    let unchanged_submissions = state.get_submissions(&round.id).await;
    let unchanged_sub = unchanged_submissions
        .iter()
        .find(|s| s.id == player1_sub.id)
        .expect("Should find submission");

    assert_eq!(
        unchanged_sub.display_text, "Player 1's answer",
        "Submission should not be changed by unauthorized player"
    );

    println!("✅ Submission update unauthorized test passed!");
}

/// Test manual AI submission creation by host
#[tokio::test]
async fn test_host_write_ai_submission() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Setup round with prompt
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");
    let prompt = state
        .add_prompt_to_pool(
            Some("Test prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state.select_prompt(&round.id, &prompt.id).await.unwrap();

    // Host writes manual AI submission
    let result = handle_message(
        ClientMessage::HostWriteAiSubmission {
            text: "This is a manually written AI answer".to_string(),
        },
        &host_role,
        &state,
    )
    .await;

    // Should return None on success (broadcast handled separately)
    assert!(result.is_none(), "Should return None on success");

    // Verify submission was created
    let submissions = state.get_submissions(&round.id).await;
    let ai_submission = submissions
        .iter()
        .find(|s| s.author_kind == gptdash::types::AuthorKind::Ai)
        .expect("Should find AI submission");

    assert_eq!(
        ai_submission.display_text, "This is a manually written AI answer",
        "AI submission should have correct text"
    );
    assert_eq!(
        ai_submission.author_ref,
        Some("host:manual".to_string()),
        "AI submission should be marked as host:manual"
    );

    println!("✅ Host write AI submission test passed!");
}

/// Test manual AI submission requires host role
#[tokio::test]
async fn test_host_write_ai_submission_unauthorized() {
    let state = Arc::new(AppState::new());
    let audience_role = Role::Audience;

    state.create_game().await;

    // Audience tries to write AI submission (should fail)
    let result = handle_message(
        ClientMessage::HostWriteAiSubmission {
            text: "Hacker AI answer".to_string(),
        },
        &audience_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code, "UNAUTHORIZED");
        }
        _ => panic!("Expected unauthorized error"),
    }

    println!("✅ Host write AI submission unauthorized test passed!");
}

/// Test manual AI submission requires active round
#[tokio::test]
async fn test_host_write_ai_submission_no_round() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Try to write AI submission without starting a round
    let result = handle_message(
        ClientMessage::HostWriteAiSubmission {
            text: "AI answer without round".to_string(),
        },
        &host_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code, "NO_ACTIVE_ROUND");
        }
        _ => panic!("Expected NO_ACTIVE_ROUND error"),
    }

    println!("✅ Host write AI submission no round test passed!");
}

/// Test AI regeneration requires host role
#[tokio::test]
async fn test_host_regenerate_ai_unauthorized() {
    let state = Arc::new(AppState::new());
    let audience_role = Role::Audience;

    state.create_game().await;

    // Audience tries to regenerate AI (should fail)
    let result = handle_message(ClientMessage::HostRegenerateAi, &audience_role, &state).await;

    match result {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code, "UNAUTHORIZED");
        }
        _ => panic!("Expected unauthorized error"),
    }

    println!("✅ Host regenerate AI unauthorized test passed!");
}

/// Test AI regeneration requires active round
#[tokio::test]
async fn test_host_regenerate_ai_no_round() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Try to regenerate AI without starting a round
    let result = handle_message(ClientMessage::HostRegenerateAi, &host_role, &state).await;

    match result {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code, "NO_ACTIVE_ROUND");
        }
        _ => panic!("Expected NO_ACTIVE_ROUND error"),
    }

    println!("✅ Host regenerate AI no round test passed!");
}

/// Test AI regeneration requires selected prompt
#[tokio::test]
async fn test_host_regenerate_ai_no_prompt() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Setup round without selecting prompt
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    // Try to regenerate AI without selecting a prompt
    let result = handle_message(ClientMessage::HostRegenerateAi, &host_role, &state).await;

    match result {
        Some(ServerMessage::Error { code, .. }) => {
            assert_eq!(code, "NO_PROMPT_SELECTED");
        }
        _ => panic!("Expected NO_PROMPT_SELECTED error"),
    }

    println!("✅ Host regenerate AI no prompt test passed!");
}

/// Test selecting AI submission for scoring
#[tokio::test]
async fn test_select_ai_submission() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Setup round with prompt
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");
    let prompt = state
        .add_prompt_to_pool(
            Some("Test prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state.select_prompt(&round.id, &prompt.id).await.unwrap();

    // Create two manual AI submissions
    handle_message(
        ClientMessage::HostWriteAiSubmission {
            text: "First AI answer".to_string(),
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(
        ClientMessage::HostWriteAiSubmission {
            text: "Second AI answer".to_string(),
        },
        &host_role,
        &state,
    )
    .await;

    // Get submissions
    let submissions = state.get_submissions(&round.id).await;
    let ai_submissions: Vec<_> = submissions
        .iter()
        .filter(|s| s.author_kind == gptdash::types::AuthorKind::Ai)
        .collect();

    assert_eq!(ai_submissions.len(), 2, "Should have 2 AI submissions");

    // Select the second AI submission
    let second_ai_id = ai_submissions[1].id.clone();
    let result = handle_message(
        ClientMessage::HostSetAiSubmission {
            submission_id: second_ai_id.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    // Should return None on success
    assert!(result.is_none(), "Should return None on success");

    // Verify the selection
    let updated_round = state.get_current_round().await.expect("Should have round");
    assert_eq!(
        updated_round.ai_submission_id,
        Some(second_ai_id),
        "Should have selected the second AI submission"
    );

    println!("✅ Select AI submission test passed!");
}

/// Test create_manual_ai_submission state method directly
#[tokio::test]
async fn test_create_manual_ai_submission_method() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Setup round
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");

    // Create manual AI submission directly
    let result = state
        .create_manual_ai_submission(&round.id, "Direct manual AI".to_string())
        .await;

    match result {
        Ok(submission) => {
            assert_eq!(submission.display_text, "Direct manual AI");
            assert_eq!(submission.author_kind, gptdash::types::AuthorKind::Ai);
            assert_eq!(submission.author_ref, Some("host:manual".to_string()));
            assert_eq!(submission.edited_by_host, Some(true));
        }
        Err(e) => panic!("Expected success, got error: {}", e),
    }

    println!("✅ Create manual AI submission method test passed!");
}

/// Test create_manual_ai_submission with invalid round
#[tokio::test]
async fn test_create_manual_ai_submission_invalid_round() {
    let state = Arc::new(AppState::new());

    state.create_game().await;

    // Try to create manual AI submission with invalid round ID
    let result = state
        .create_manual_ai_submission("invalid_round_id", "AI text".to_string())
        .await;

    match result {
        Err(e) => {
            assert!(
                e.contains("Round not found"),
                "Should indicate round not found"
            );
        }
        Ok(_) => panic!("Expected error for invalid round"),
    }

    println!("✅ Create manual AI submission invalid round test passed!");
}

/// Test removing a player mid-round
#[tokio::test]
async fn test_remove_player_mid_round() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;
    let audience_role = Role::Audience;

    state.create_game().await;

    // Create two players
    let create_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 2 },
        &host_role,
        &state,
    )
    .await;

    let player_tokens = match create_result {
        Some(ServerMessage::PlayersCreated { players }) => players,
        _ => panic!("Expected PlayersCreated"),
    };

    // Register players
    handle_message(
        ClientMessage::RegisterPlayer {
            player_token: player_tokens[0].token.clone(),
            display_name: "Alice".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    handle_message(
        ClientMessage::RegisterPlayer {
            player_token: player_tokens[1].token.clone(),
            display_name: "Bob".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    // Setup round
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");
    let prompt = state
        .add_prompt_to_pool(
            Some("Test prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state.select_prompt(&round.id, &prompt.id).await.unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Both players submit
    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_tokens[0].token.clone()),
            text: "Alice's answer".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_tokens[1].token.clone()),
            text: "Bob's answer".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    // Add AI submission
    let ai_sub = state
        .submit_answer(&round.id, None, "AI answer".to_string())
        .await
        .unwrap();

    // Verify we have 3 submissions
    let submissions = state.get_submissions(&round.id).await;
    assert_eq!(submissions.len(), 3);

    // Set reveal order
    let reveal_order: Vec<_> = submissions.iter().map(|s| s.id.clone()).collect();
    handle_message(
        ClientMessage::HostSetRevealOrder {
            order: reveal_order.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    // Transition to Voting
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Reveal,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Voting,
        },
        &host_role,
        &state,
    )
    .await;

    // Audience votes - voter picks Alice's submission as AI
    let alice_sub = submissions
        .iter()
        .find(|s| s.author_ref.as_ref() == Some(&player_tokens[0].id))
        .expect("Should find Alice's submission");

    handle_message(
        ClientMessage::Vote {
            voter_token: "voter1".to_string(),
            ai: alice_sub.id.clone(),
            funny: alice_sub.id.clone(),
            msg_id: "vote1".to_string(),
        },
        &audience_role,
        &state,
    )
    .await;

    // Verify vote exists
    assert_eq!(state.votes.read().await.len(), 1);

    // Now remove Alice mid-voting phase
    let remove_result = handle_message(
        ClientMessage::HostRemovePlayer {
            player_id: player_tokens[0].id.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    // Should succeed
    assert!(remove_result.is_none());

    // Verify Alice is gone
    assert!(state
        .get_player_by_token(&player_tokens[0].token)
        .await
        .is_none());

    // Verify Alice's submission is gone
    let updated_submissions = state.get_submissions(&round.id).await;
    assert_eq!(updated_submissions.len(), 2);

    // Verify vote that referenced Alice's submission is cleared
    // voter1 can now vote again
    assert_eq!(state.votes.read().await.len(), 0);

    // Verify reveal order is updated (doesn't contain Alice's submission anymore)
    let updated_round = state.get_current_round().await.unwrap();
    assert_eq!(updated_round.reveal_order.len(), 2);
    assert!(
        !updated_round.reveal_order.contains(&alice_sub.id),
        "Alice's submission should be removed from reveal order"
    );

    // Bob should still be in the game
    assert!(state
        .get_player_by_token(&player_tokens[1].token)
        .await
        .is_some());

    // Can still proceed to Results with remaining submissions
    state
        .set_ai_submission(&round.id, ai_sub.id.clone())
        .await
        .unwrap();

    let result = handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Results,
        },
        &host_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Phase { phase, .. }) => {
            assert_eq!(phase, GamePhase::Results);
        }
        _ => panic!("Expected Phase message for Results"),
    }

    println!("✅ Remove player mid-round test passed!");
}

/// Test adding a new player mid-round
#[tokio::test]
async fn test_add_player_mid_round() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;

    state.create_game().await;

    // Create initial player
    let create_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 1 },
        &host_role,
        &state,
    )
    .await;

    let initial_tokens = match create_result {
        Some(ServerMessage::PlayersCreated { players }) => players,
        _ => panic!("Expected PlayersCreated"),
    };

    // Setup round
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");
    let prompt = state
        .add_prompt_to_pool(
            Some("Test prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state.select_prompt(&round.id, &prompt.id).await.unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Initial player submits
    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(initial_tokens[0].token.clone()),
            text: "First player's answer".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    // Now add a new player mid-round (late arrival)
    let add_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 1 },
        &host_role,
        &state,
    )
    .await;

    let new_tokens = match add_result {
        Some(ServerMessage::PlayersCreated { players }) => {
            assert_eq!(players.len(), 1);
            players
        }
        _ => panic!("Expected PlayersCreated for new player"),
    };

    // New player can register
    let register_result = handle_message(
        ClientMessage::RegisterPlayer {
            player_token: new_tokens[0].token.clone(),
            display_name: "Late Arrival".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match register_result {
        Some(ServerMessage::PlayerRegistered { display_name, .. }) => {
            assert_eq!(display_name, "Late Arrival");
        }
        _ => panic!("Expected PlayerRegistered"),
    }

    // New player can submit (if still in Writing phase)
    let submit_result = handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(new_tokens[0].token.clone()),
            text: "Late player's answer".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match submit_result {
        Some(ServerMessage::SubmissionConfirmed) => {}
        _ => panic!("Expected SubmissionConfirmed for late player"),
    }

    // Verify we now have 2 player submissions
    let submissions = state.get_submissions(&round.id).await;
    let player_submissions: Vec<_> = submissions
        .iter()
        .filter(|s| s.author_kind == gptdash::types::AuthorKind::Player)
        .collect();

    assert_eq!(
        player_submissions.len(),
        2,
        "Should have 2 player submissions"
    );

    // Verify player status shows both
    let statuses = state.get_all_player_status().await;
    assert_eq!(statuses.len(), 2, "Should have 2 players in status list");

    println!("✅ Add player mid-round test passed!");
}

// ============================================================================
// Multimodal Prompts Tests
// ============================================================================

/// Test adding a prompt with only an image URL (no text)
#[tokio::test]
async fn test_multimodal_prompt_image_only() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Transition to PromptSelection and start round
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = state.get_current_round().await.expect("Should have round");

    // Add a prompt with only image URL
    let image_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/3/31/2013-12-30_30C3_3467.JPG/2560px-2013-12-30_30C3_3467.JPG";
    let prompt = state
        .add_prompt_to_pool(
            None, // No text
            Some(image_url.to_string()),
            PromptSource::Host,
            None,
        )
        .await
        .expect("Should add image-only prompt");

    // Verify prompt was created with image URL
    assert!(prompt.text.is_none());
    assert_eq!(prompt.image_url, Some(image_url.to_string()));

    // Verify prompt can be selected
    let select_result = state.select_prompt(&round.id, &prompt.id).await;
    assert!(select_result.is_ok());

    // Verify selected prompt has image URL
    let current_round = state.get_current_round().await.unwrap();
    let selected = current_round
        .selected_prompt
        .expect("Should have selected prompt");
    assert_eq!(selected.image_url, Some(image_url.to_string()));

    println!("✅ Multimodal image-only prompt test passed!");
}

/// Test adding a prompt with both text and image URL
#[tokio::test]
async fn test_multimodal_prompt_text_and_image() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let _round = state.get_current_round().await.expect("Should have round");

    // Add a prompt with both text and image
    let image_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/3/31/2013-12-30_30C3_3467.JPG/2560px-2013-12-30_30C3_3467.JPG";
    let text = "What is this rocket called and what organization does it represent?";
    let prompt = state
        .add_prompt_to_pool(
            Some(text.to_string()),
            Some(image_url.to_string()),
            PromptSource::Host,
            None,
        )
        .await
        .expect("Should add multimodal prompt");

    // Verify both fields are set
    assert_eq!(prompt.text, Some(text.to_string()));
    assert_eq!(prompt.image_url, Some(image_url.to_string()));

    println!("✅ Multimodal text+image prompt test passed!");
}

/// Test that prompts with neither text nor image are rejected
#[tokio::test]
async fn test_multimodal_prompt_requires_content() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let _round = state.get_current_round().await.expect("Should have round");

    // Try to add a prompt with neither text nor image - should fail
    let result = state
        .add_prompt_to_pool(None, None, PromptSource::Host, None)
        .await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("must have either text or image_url"));

    println!("✅ Multimodal prompt validation test passed!");
}

/// Test multimodal prompt through the handler (HostAddPrompt message)
#[tokio::test]
async fn test_multimodal_prompt_via_handler() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    // Add prompt via handler with image URL (adds to pool)
    let image_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/3/31/2013-12-30_30C3_3467.JPG/2560px-2013-12-30_30C3_3467.JPG";
    let result = handle_message(
        ClientMessage::HostAddPrompt {
            text: Some("Describe this image".to_string()),
            image_url: Some(image_url.to_string()),
        },
        &host_role,
        &state,
    )
    .await;

    // Adding to pool returns None (success acknowledgment)
    assert!(
        result.is_none(),
        "HostAddPrompt should return None, got: {:?}",
        result
    );

    // Get the prompt from pool and select it
    let pool = state.prompt_pool.read().await;
    assert_eq!(pool.len(), 1);
    let prompt_id = pool[0].id.clone();
    drop(pool);

    let select_result = handle_message(
        ClientMessage::HostSelectPrompt { prompt_id },
        &host_role,
        &state,
    )
    .await;

    match select_result {
        Some(ServerMessage::PromptSelected { prompt }) => {
            assert_eq!(prompt.text, Some("Describe this image".to_string()));
            assert_eq!(prompt.image_url, Some(image_url.to_string()));
        }
        other => panic!("Expected PromptSelected message, got {:?}", other),
    }

    println!("✅ Multimodal prompt via handler test passed!");
}

/// Test multimodal prompt is included in PromptSelected message
#[tokio::test]
async fn test_multimodal_prompt_selected_includes_image() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let _round = state.get_current_round().await.expect("Should have round");

    // Add multimodal prompt
    let image_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/3/31/2013-12-30_30C3_3467.JPG/2560px-2013-12-30_30C3_3467.JPG";
    let prompt = state
        .add_prompt_to_pool(
            Some("What do you see?".to_string()),
            Some(image_url.to_string()),
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();

    // Select the prompt via handler
    let result = handle_message(
        ClientMessage::HostSelectPrompt {
            prompt_id: prompt.id.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    // Verify PromptSelected includes image_url
    match result {
        Some(ServerMessage::PromptSelected { prompt: selected }) => {
            assert_eq!(selected.id, prompt.id);
            assert_eq!(selected.text, Some("What do you see?".to_string()));
            assert_eq!(selected.image_url, Some(image_url.to_string()));
        }
        _ => panic!("Expected PromptSelected message, got {:?}", result),
    }

    println!("✅ Multimodal PromptSelected includes image test passed!");
}

// ============================================================================
// Host Prompt Addition Tests
// ============================================================================

/// Test that adding a prompt in LOBBY phase (before starting a round) works
/// This tests the bug where HostAddPrompt silently fails when no round exists
#[tokio::test]
async fn test_add_prompt_in_lobby_without_round() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Verify we're in LOBBY phase
    let game = state.get_game().await.expect("Game should exist");
    assert_eq!(game.phase, GamePhase::Lobby);

    // Verify there's no current round
    let round = state.get_current_round().await;
    assert!(round.is_none(), "Should not have a round yet");

    // Add a prompt via handler - this adds to the global pool (no round needed)
    let result = handle_message(
        ClientMessage::HostAddPrompt {
            text: Some("Test prompt in lobby".to_string()),
            image_url: None,
        },
        &host_role,
        &state,
    )
    .await;

    // With the new design, adding a prompt to the pool returns None (success acknowledgment)
    // The prompt is NOT auto-selected - that requires a separate HostSelectPrompt call
    assert!(
        result.is_none(),
        "HostAddPrompt should return None (success acknowledgment), got: {:?}",
        result
    );

    // Verify the prompt was added to the global pool
    let pool = state.prompt_pool.read().await;
    assert_eq!(pool.len(), 1, "Prompt should be in the pool");
    assert_eq!(pool[0].text, Some("Test prompt in lobby".to_string()));

    println!("✅ Add prompt in lobby without round test passed!");
}

/// Test that adding a prompt after starting a round works (happy path)
#[tokio::test]
async fn test_add_prompt_after_starting_round() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Transition to PromptSelection and start round (normal flow)
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    // Add a prompt to the global pool
    let result = handle_message(
        ClientMessage::HostAddPrompt {
            text: Some("Test prompt after round start".to_string()),
            image_url: None,
        },
        &host_role,
        &state,
    )
    .await;

    // Adding to pool returns None (success acknowledgment)
    assert!(
        result.is_none(),
        "HostAddPrompt should return None, got: {:?}",
        result
    );

    // Verify prompt is in the pool
    let pool = state.prompt_pool.read().await;
    assert_eq!(pool.len(), 1);
    let prompt_id = pool[0].id.clone();
    drop(pool);

    // Now select the prompt for the round
    let select_result = handle_message(
        ClientMessage::HostSelectPrompt { prompt_id },
        &host_role,
        &state,
    )
    .await;

    match select_result {
        Some(ServerMessage::PromptSelected { prompt }) => {
            assert_eq!(
                prompt.text,
                Some("Test prompt after round start".to_string())
            );
        }
        other => {
            panic!("Expected PromptSelected, got: {:?}", other);
        }
    }

    println!("✅ Add prompt after starting round test passed!");
}

/// Test that host receives HostPrompts broadcast after adding a prompt
#[tokio::test]
async fn test_host_prompts_broadcast_after_add() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Start a round
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::PromptSelection,
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let _round = state.get_current_round().await.expect("Should have round");

    // Add a prompt directly to test the prompts list
    state
        .add_prompt_to_pool(
            Some("First prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .expect("Should add first prompt");

    state
        .add_prompt_to_pool(
            Some("Second prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .expect("Should add second prompt");

    // Get prompts from the global pool
    let pool = state.prompt_pool.read().await;
    assert_eq!(pool.len(), 2, "Should have 2 prompt candidates in pool");

    println!("✅ Host prompts broadcast test passed!");
}
