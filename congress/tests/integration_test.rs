use gptdash::protocol::{ClientMessage, PlayerSubmissionStatus, ServerMessage};
use gptdash::state::AppState;
use gptdash::types::{GamePhase, PromptSource, Role};
use gptdash::ws::handlers::handle_message;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Compute the challenge response for a vote (same algorithm as client)
fn compute_challenge_response(nonce: &str, voter_token: &str) -> String {
    let input = format!("{}{}", nonce, voter_token);
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(&hash[..8]) // First 8 bytes = 16 hex chars
}

/// End-to-end integration test for a complete game flow
#[tokio::test]
async fn test_full_game_flow() {
    // Skip anti-automation checks (webdriver + timing) for this test
    std::env::set_var("SKIP_VOTE_ANTI_AUTOMATION", "1");

    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;
    let audience_role = Role::Audience;

    // 1. Setup: Create game
    state.create_game().await;
    let game = state.get_game().await.expect("Game should exist");
    assert_eq!(game.phase, GamePhase::Lobby);

    // 1b. Create audience members (required for voter validation)
    state.get_or_create_audience_member("voter_1").await;
    state.get_or_create_audience_member("voter_2").await;

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

    // 4. Start round (from Lobby)
    let start_round_result =
        handle_message(ClientMessage::HostStartRound, &host_role, &state).await;

    let round = match start_round_result {
        Some(ServerMessage::RoundStarted { round }) => {
            assert_eq!(round.number, 1);
            round
        }
        _ => panic!("Expected RoundStarted message"),
    };

    // 5. Add and select prompt
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
            model: None,
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

    // 6. Transition to Writing phase (directly from Lobby)
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

    // Get the challenge nonce (generated when entering VOTING phase)
    let challenge_nonce = state
        .get_vote_challenge_nonce()
        .await
        .expect("Challenge nonce should exist in VOTING phase");

    let vote_result = handle_message(
        ClientMessage::Vote {
            voter_token: "voter_1".to_string(),
            ai: ai_submission.id.clone(),
            funny: player_submission.id.clone(),
            msg_id: "vote_1".to_string(),
            challenge_nonce: challenge_nonce.clone(),
            challenge_response: compute_challenge_response(&challenge_nonce, "voter_1"),
            is_webdriver: false,
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
            challenge_nonce: challenge_nonce.clone(),
            challenge_response: compute_challenge_response(&challenge_nonce, "voter_2"),
            is_webdriver: false,
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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

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
    let result = handle_message(
        ClientMessage::HostRegenerateAi { model: None },
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

    println!("✅ Host regenerate AI unauthorized test passed!");
}

/// Test AI regeneration requires active round
#[tokio::test]
async fn test_host_regenerate_ai_no_round() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Try to regenerate AI without starting a round
    let result = handle_message(
        ClientMessage::HostRegenerateAi { model: None },
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
    let result = handle_message(
        ClientMessage::HostRegenerateAi { model: None },
        &host_role,
        &state,
    )
    .await;

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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

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
    // Skip anti-automation checks (webdriver + timing) for this test
    std::env::set_var("SKIP_VOTE_ANTI_AUTOMATION", "1");

    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;
    let audience_role = Role::Audience;

    state.create_game().await;

    // Create audience member (required for voter validation)
    state.get_or_create_audience_member("voter1").await;

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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

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

    // Get the challenge nonce (generated when entering VOTING phase)
    let challenge_nonce = state
        .get_vote_challenge_nonce()
        .await
        .expect("Challenge nonce should exist in VOTING phase");

    handle_message(
        ClientMessage::Vote {
            voter_token: "voter1".to_string(),
            ai: alice_sub.id.clone(),
            funny: alice_sub.id.clone(),
            msg_id: "vote1".to_string(),
            challenge_nonce: challenge_nonce.clone(),
            challenge_response: compute_challenge_response(&challenge_nonce, "voter1"),
            is_webdriver: false,
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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

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
    let select_result = state.select_prompt(&round.id, &prompt.id, None).await;
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
    let prompt_id = pool.values().next().unwrap().id.clone();
    drop(pool);

    let select_result = handle_message(
        ClientMessage::HostSelectPrompt {
            prompt_id,
            model: None,
        },
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
            model: None,
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
    assert_eq!(
        pool.values().next().unwrap().text,
        Some("Test prompt in lobby".to_string())
    );

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
    let prompt_id = pool.values().next().unwrap().id.clone();
    drop(pool);

    // Now select the prompt for the round
    let select_result = handle_message(
        ClientMessage::HostSelectPrompt {
            prompt_id,
            model: None,
        },
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

// ============================================================================
// Player Token Validation Tests
// ============================================================================

/// Test that registering with an invalid player token returns an error
#[tokio::test]
async fn test_register_player_invalid_token() {
    let state = Arc::new(AppState::new());
    let player_role = Role::Player;

    state.create_game().await;

    // Try to register with a token that doesn't exist
    let result = handle_message(
        ClientMessage::RegisterPlayer {
            player_token: "INVALID_TOKEN".to_string(),
            display_name: "Hacker".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, msg }) => {
            assert_eq!(code, "REGISTRATION_FAILED");
            assert!(msg.contains("Invalid player token"));
        }
        _ => panic!("Expected error for invalid player token, got: {:?}", result),
    }

    println!("✅ Register player invalid token test passed!");
}

/// Test that submitting answers with an invalid player token returns an error
#[tokio::test]
async fn test_submit_answer_invalid_token() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;

    state.create_game().await;

    // Setup a round with prompt so submissions are allowed
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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Try to submit an answer with an invalid token
    let result = handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some("INVALID_TOKEN".to_string()),
            text: "My cheating answer".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::Error { code, msg }) => {
            assert_eq!(code, "INVALID_PLAYER_TOKEN");
            assert!(msg.contains("Invalid player token"));
        }
        _ => panic!(
            "Expected INVALID_PLAYER_TOKEN error for invalid token, got: {:?}",
            result
        ),
    }

    // Verify no submission was created
    let submissions = state.get_submissions(&round.id).await;
    assert!(
        submissions.is_empty(),
        "No submissions should exist from invalid token"
    );

    println!("✅ Submit answer invalid token test passed!");
}

/// Test that a valid player token works for registration and submission
#[tokio::test]
async fn test_valid_player_token_flow() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;

    state.create_game().await;

    // Create a valid player
    let create_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 1 },
        &host_role,
        &state,
    )
    .await;

    let player_token = match create_result {
        Some(ServerMessage::PlayersCreated { players }) => {
            assert_eq!(players.len(), 1);
            players[0].token.clone()
        }
        _ => panic!("Expected PlayersCreated"),
    };

    // Register with valid token - should succeed
    let register_result = handle_message(
        ClientMessage::RegisterPlayer {
            player_token: player_token.clone(),
            display_name: "ValidPlayer".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match register_result {
        Some(ServerMessage::PlayerRegistered { display_name, .. }) => {
            assert_eq!(display_name, "ValidPlayer");
        }
        _ => panic!(
            "Expected PlayerRegistered for valid token, got: {:?}",
            register_result
        ),
    }

    // Setup round for submission
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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Submit with valid token - should succeed
    let submit_result = handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_token),
            text: "My valid answer".to_string(),
        },
        &player_role,
        &state,
    )
    .await;

    match submit_result {
        Some(ServerMessage::SubmissionConfirmed) => {}
        _ => panic!(
            "Expected SubmissionConfirmed for valid token, got: {:?}",
            submit_result
        ),
    }

    // Verify submission was created
    let submissions = state.get_submissions(&round.id).await;
    assert_eq!(submissions.len(), 1, "Should have 1 submission");
    assert_eq!(submissions[0].display_text, "My valid answer");

    println!("✅ Valid player token flow test passed!");
}

// ============================================================================
// Auto-Save/Load Tests
// ============================================================================

/// Test that state can be saved to a file and loaded back
#[tokio::test]
async fn test_auto_save_writes_state_to_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let save_path = temp_dir.path().join("test_state.json");

    let state = Arc::new(AppState::new());
    state.create_game().await;

    // Add some identifiable data
    let prompt = state
        .add_prompt_to_pool(
            Some("Test prompt for auto-save".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .expect("Should add prompt");

    // Manually trigger save
    gptdash::broadcast::save_state_to_file(&state, &save_path)
        .await
        .expect("Should save state to file");

    // Verify file exists and contains valid JSON
    let contents = tokio::fs::read_to_string(&save_path)
        .await
        .expect("Should read file");
    let export: gptdash::state::export::GameStateExport =
        serde_json::from_str(&contents).expect("Should parse JSON");

    // Verify game state was saved
    assert!(export.game.is_some(), "Should have game in export");

    // Verify prompt pool was saved
    assert_eq!(export.prompt_pool.len(), 1, "Should have 1 prompt in pool");
    assert_eq!(export.prompt_pool[0].id, prompt.id);

    println!("✅ Auto-save writes state to file test passed!");
}

/// Test that state can be loaded from a file
#[tokio::test]
async fn test_auto_load_restores_state_from_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let save_path = temp_dir.path().join("test_state.json");

    // Create and save initial state with identifiable data
    let state1 = Arc::new(AppState::new());
    state1.create_game().await;

    // Add a prompt with unique text
    state1
        .add_prompt_to_pool(
            Some("Unique prompt for restore test".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .expect("Should add prompt");

    // Create a player
    let player = state1.create_player().await;
    state1
        .register_player(&player.token, "TestPlayer".to_string())
        .await
        .expect("Should register player");

    // Save state
    gptdash::broadcast::save_state_to_file(&state1, &save_path)
        .await
        .expect("Should save state");

    // Create new state and load from file
    let state2 = Arc::new(AppState::new());
    gptdash::broadcast::load_state_from_file(&state2, &save_path)
        .await
        .expect("Should load state");

    // Verify game was restored
    assert!(
        state2.get_game().await.is_some(),
        "Should have game after load"
    );

    // Verify prompt pool was restored
    let pool = state2.prompt_pool.read().await;
    assert_eq!(pool.len(), 1, "Should have 1 prompt after restore");
    assert_eq!(
        pool.values().next().unwrap().text,
        Some("Unique prompt for restore test".to_string())
    );
    drop(pool);

    // Verify player was restored
    let restored_player = state2.get_player_by_token(&player.token).await;
    assert!(
        restored_player.is_some(),
        "Player should be restored with token"
    );
    assert_eq!(
        restored_player.unwrap().display_name,
        Some("TestPlayer".to_string())
    );

    println!("✅ Auto-load restores state from file test passed!");
}

/// Test that load fails gracefully for non-existent file
#[tokio::test]
async fn test_auto_load_nonexistent_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let save_path = temp_dir.path().join("nonexistent.json");

    let state = Arc::new(AppState::new());

    let result = gptdash::broadcast::load_state_from_file(&state, &save_path).await;

    assert!(result.is_err(), "Should fail for non-existent file");
    assert!(
        result.unwrap_err().contains("Failed to read"),
        "Error should mention read failure"
    );

    println!("✅ Auto-load nonexistent file test passed!");
}

/// Test that load fails gracefully for invalid JSON
#[tokio::test]
async fn test_auto_load_invalid_json() {
    let temp_dir = tempfile::tempdir().unwrap();
    let save_path = temp_dir.path().join("invalid.json");

    // Write invalid JSON to file
    tokio::fs::write(&save_path, "{ invalid json }")
        .await
        .expect("Should write file");

    let state = Arc::new(AppState::new());

    let result = gptdash::broadcast::load_state_from_file(&state, &save_path).await;

    assert!(result.is_err(), "Should fail for invalid JSON");
    assert!(
        result.unwrap_err().contains("Failed to parse"),
        "Error should mention parse failure"
    );

    println!("✅ Auto-load invalid JSON test passed!");
}

// ============================================================================
// Panic Mode Tests
// ============================================================================

/// Test that panic mode sends disconnect signal to audience connections
#[tokio::test]
async fn test_panic_mode_sends_disconnect_signal() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Subscribe to the disconnect signal before enabling panic mode
    let mut disconnect_rx = state.audience_disconnect.subscribe();

    // Enable panic mode
    handle_message(
        ClientMessage::HostTogglePanicMode { enabled: true },
        &host_role,
        &state,
    )
    .await;

    // Verify panic mode is active
    assert!(state.is_panic_mode().await, "Panic mode should be enabled");

    // Verify disconnect signal was sent
    let result = disconnect_rx.try_recv();
    assert!(
        result.is_ok(),
        "Should have received disconnect signal, got: {:?}",
        result
    );

    println!("✅ Panic mode sends disconnect signal test passed!");
}

/// Test that disabling panic mode does NOT send disconnect signal
#[tokio::test]
async fn test_panic_mode_disable_no_disconnect() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Enable panic mode first
    state.set_panic_mode(true).await;

    // Subscribe to the disconnect signal
    let mut disconnect_rx = state.audience_disconnect.subscribe();

    // Disable panic mode
    handle_message(
        ClientMessage::HostTogglePanicMode { enabled: false },
        &host_role,
        &state,
    )
    .await;

    // Verify panic mode is disabled
    assert!(
        !state.is_panic_mode().await,
        "Panic mode should be disabled"
    );

    // Verify no disconnect signal was sent (disabling shouldn't disconnect)
    let result = disconnect_rx.try_recv();
    assert!(
        result.is_err(),
        "Should NOT receive disconnect signal when disabling panic mode"
    );

    println!("✅ Panic mode disable no disconnect test passed!");
}

/// Test that manual winners are broadcast when entering Results phase
#[tokio::test]
async fn test_manual_winners_broadcast() {
    // Skip anti-automation checks
    std::env::set_var("SKIP_VOTE_ANTI_AUTOMATION", "1");

    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let player_role = Role::Player;

    state.create_game().await;

    // Create a player
    let create_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 1 },
        &host_role,
        &state,
    )
    .await;

    let player_tokens = match create_result {
        Some(ServerMessage::PlayersCreated { players }) => players,
        _ => panic!("Expected PlayersCreated"),
    };

    // Register player
    handle_message(
        ClientMessage::RegisterPlayer {
            player_token: player_tokens[0].token.clone(),
            display_name: "Alice".to_string(),
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
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Player submits
    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_tokens[0].token.clone()),
            text: "Alice's answer".to_string(),
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

    // Set the official AI submission
    handle_message(
        ClientMessage::HostSetAiSubmission {
            submission_id: ai_sub.id.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    // Get player submission
    let submissions = state.get_submissions(&round.id).await;
    let player_sub = submissions
        .iter()
        .find(|s| s.author_kind == gptdash::types::AuthorKind::Player)
        .expect("Should find player submission");

    // Set reveal order and go to Reveal -> Voting
    let reveal_order: Vec<_> = submissions.iter().map(|s| s.id.clone()).collect();
    handle_message(
        ClientMessage::HostSetRevealOrder {
            order: reveal_order,
        },
        &host_role,
        &state,
    )
    .await;

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

    // Enable panic mode (simulating audience voting failure)
    state.set_panic_mode(true).await;

    // Set manual winners
    handle_message(
        ClientMessage::HostSetManualWinner {
            winner_type: gptdash::protocol::ManualWinnerType::Ai,
            submission_id: ai_sub.id.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    handle_message(
        ClientMessage::HostSetManualWinner {
            winner_type: gptdash::protocol::ManualWinnerType::Funny,
            submission_id: player_sub.id.clone(),
        },
        &host_role,
        &state,
    )
    .await;

    // Verify manual winners are set in the round
    let updated_round = state.get_current_round().await.unwrap();
    assert_eq!(
        updated_round.manual_ai_winner,
        Some(ai_sub.id.clone()),
        "Manual AI winner should be set"
    );
    assert_eq!(
        updated_round.manual_funny_winner,
        Some(player_sub.id.clone()),
        "Manual funny winner should be set"
    );

    // Subscribe to broadcast to verify ManualWinners message is sent
    let mut broadcast_rx = state.broadcast.subscribe();

    // Transition to Results (this should broadcast ManualWinners)
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Results,
        },
        &host_role,
        &state,
    )
    .await;

    // Check for ManualWinners message in broadcast
    let mut found_manual_winners = false;
    while let Ok(msg) = broadcast_rx.try_recv() {
        if let ServerMessage::ManualWinners {
            ai_winner_id,
            funny_winner_id,
        } = msg
        {
            assert_eq!(ai_winner_id, Some(ai_sub.id.clone()));
            assert_eq!(funny_winner_id, Some(player_sub.id.clone()));
            found_manual_winners = true;
            break;
        }
    }

    assert!(
        found_manual_winners,
        "ManualWinners message should be broadcast when entering Results"
    );

    println!("✅ Manual winners broadcast test passed!");
}

/// Test that panic mode is preserved through state export/import
#[tokio::test]
async fn test_panic_mode_state_persistence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let save_path = temp_dir.path().join("panic_state.json");

    // Create state with panic mode enabled
    let state1 = Arc::new(AppState::new());
    state1.create_game().await;
    state1.set_panic_mode(true).await;

    // Verify panic mode is on
    assert!(state1.is_panic_mode().await);

    // Save state
    gptdash::broadcast::save_state_to_file(&state1, &save_path)
        .await
        .expect("Should save state");

    // Load into new state
    let state2 = Arc::new(AppState::new());
    gptdash::broadcast::load_state_from_file(&state2, &save_path)
        .await
        .expect("Should load state");

    // Verify panic mode was restored
    assert!(
        state2.is_panic_mode().await,
        "Panic mode should be preserved after state restore"
    );

    println!("✅ Panic mode state persistence test passed!");
}

// ============================================================================
// Prompt Selection Tests (Multiple Prompts with Audience Voting)
// ============================================================================

/// Test PROMPT_SELECTION with 2 prompts and audience voting
#[tokio::test]
async fn test_prompt_selection_audience_voting() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Add 2 distinct prompts to pool and queue them
    let prompt1 = state
        .add_prompt_to_pool(
            Some("What is artificial intelligence?".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    let prompt2 = state
        .add_prompt_to_pool(
            Some("Describe your favorite animal".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();

    state.queue_prompt(&prompt1.id).await.unwrap();
    state.queue_prompt(&prompt2.id).await.unwrap();

    // Verify we have 2 queued prompts
    assert_eq!(state.queued_prompts.read().await.len(), 2);

    // Transition to PROMPT_SELECTION - should NOT auto-advance with 2 prompts
    state
        .transition_phase(GamePhase::PromptSelection)
        .await
        .unwrap();

    let game = state.get_game().await.unwrap();
    assert_eq!(
        game.phase,
        GamePhase::PromptSelection,
        "Should stay in PROMPT_SELECTION with 2+ prompts"
    );

    // Audience votes: 3 votes for prompt2, 1 vote for prompt1
    state
        .record_prompt_vote("voter1", &prompt2.id)
        .await
        .unwrap();
    state
        .record_prompt_vote("voter2", &prompt2.id)
        .await
        .unwrap();
    state
        .record_prompt_vote("voter3", &prompt2.id)
        .await
        .unwrap();
    state
        .record_prompt_vote("voter4", &prompt1.id)
        .await
        .unwrap();

    // Verify vote counts
    let counts = state.get_prompt_vote_counts().await;
    assert_eq!(counts.get(&prompt2.id), Some(&3));
    assert_eq!(counts.get(&prompt1.id), Some(&1));

    // Transition to WRITING - this should select the winning prompt
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Verify winning prompt (prompt2) was selected
    let round = state.get_current_round().await.expect("Should have round");
    let selected = round.selected_prompt.expect("Should have selected prompt");
    assert_eq!(
        selected.id, prompt2.id,
        "Prompt with most votes should be selected"
    );

    // Verify losing prompt was returned to pool
    let pool = state.prompt_pool.read().await;
    assert!(
        pool.contains_key(&prompt1.id),
        "Losing prompt should be returned to pool"
    );
    assert!(
        !pool.contains_key(&prompt2.id),
        "Winning prompt should not be in pool"
    );

    // Verify prompt_votes were cleared
    assert!(
        state.prompt_votes.read().await.is_empty(),
        "Prompt votes should be cleared after selection"
    );

    println!("✅ Prompt selection audience voting test passed!");
}

/// Test PROMPT_SELECTION tie resolution (last queued prompt wins due to max_by_key behavior)
#[tokio::test]
async fn test_prompt_selection_tie_resolution() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Add 2 VERY DISTINCT prompts to avoid fuzzy deduplication
    let prompt1 = state
        .add_prompt_to_pool(
            Some("What is the capital of France?".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    let prompt2 = state
        .add_prompt_to_pool(
            Some("Describe your favorite pizza topping".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();

    // Queue prompt1 first, then prompt2
    state.queue_prompt(&prompt1.id).await.unwrap();
    state.queue_prompt(&prompt2.id).await.unwrap();

    // Transition to PROMPT_SELECTION
    state
        .transition_phase(GamePhase::PromptSelection)
        .await
        .unwrap();

    // Equal votes: 1 vote each (tie scenario)
    state
        .record_prompt_vote("voter1", &prompt1.id)
        .await
        .unwrap();
    state
        .record_prompt_vote("voter2", &prompt2.id)
        .await
        .unwrap();

    // Transition to WRITING - tie resolution uses max_by_key which returns LAST maximum
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Verify last queued prompt won (max_by_key returns last element on tie)
    let round = state.get_current_round().await.expect("Should have round");
    let selected = round.selected_prompt.expect("Should have selected prompt");
    assert_eq!(
        selected.id, prompt2.id,
        "Last queued prompt wins on tie (max_by_key behavior)"
    );

    // Verify the other prompt was returned to pool
    let pool = state.prompt_pool.read().await;
    assert!(
        pool.contains_key(&prompt1.id),
        "Losing prompt should be returned to pool"
    );

    println!("✅ Prompt selection tie resolution test passed!");
}

/// Test host can directly select a prompt from pool (bypassing queue/vote system)
#[tokio::test]
async fn test_host_direct_prompt_selection() {
    let state = Arc::new(AppState::new());
    let host_role = Role::Host;

    state.create_game().await;

    // Add a prompt to the pool (NOT queued)
    let prompt = state
        .add_prompt_to_pool(
            Some("How many planets are in the solar system?".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();

    // Host directly selects the prompt from pool (bypasses queue/vote system)
    let result = handle_message(
        ClientMessage::HostSelectPrompt {
            prompt_id: prompt.id.clone(),
            model: None,
        },
        &host_role,
        &state,
    )
    .await;

    match result {
        Some(ServerMessage::PromptSelected { prompt: selected }) => {
            assert_eq!(
                selected.id, prompt.id,
                "Host should be able to directly select prompt from pool"
            );
        }
        _ => panic!("Expected PromptSelected message"),
    }

    // Verify prompt was selected and round is ready
    let round = state.get_current_round().await.expect("Should have round");
    let selected = round.selected_prompt.expect("Should have selected prompt");
    assert_eq!(
        selected.id, prompt.id,
        "Prompt should be selected for current round"
    );

    // Verify prompt was removed from pool
    let pool = state.prompt_pool.read().await;
    assert!(
        !pool.contains_key(&prompt.id),
        "Selected prompt should be removed from pool"
    );

    println!("✅ Host direct prompt selection test passed!");
}

// ============================================================================
// Vote Timing Validation Tests (500ms Anti-Automation)
// ============================================================================

/// Test that votes submitted within 500ms of VOTING phase start are shadow-rejected
#[tokio::test]
async fn test_vote_timing_early_rejection() {
    // Ensure anti-automation is NOT disabled
    std::env::remove_var("SKIP_VOTE_ANTI_AUTOMATION");

    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let audience_role = Role::Audience;

    state.create_game().await;

    // Create player and setup round
    let create_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 1 },
        &host_role,
        &state,
    )
    .await;

    let player_tokens = match create_result {
        Some(ServerMessage::PlayersCreated { players }) => players,
        _ => panic!("Expected PlayersCreated"),
    };

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
            Some("Vote timing test prompt".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Player submits
    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_tokens[0].token.clone()),
            text: "Player answer for timing test".to_string(),
        },
        &Role::Player,
        &state,
    )
    .await;

    // Add AI submission and set reveal order
    let ai_sub = state
        .submit_answer(&round.id, None, "AI answer".to_string())
        .await
        .unwrap();
    let submissions = state.get_submissions(&round.id).await;
    let reveal_order: Vec<_> = submissions.iter().map(|s| s.id.clone()).collect();
    state
        .set_reveal_order(&round.id, reveal_order)
        .await
        .unwrap();
    state
        .set_ai_submission(&round.id, ai_sub.id.clone())
        .await
        .unwrap();

    // Transition through Reveal to Voting
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Reveal,
        },
        &host_role,
        &state,
    )
    .await;

    // Manually set voting phase start to NOW (so next vote is within 500ms)
    state.set_voting_phase_started().await;

    // Set game phase to Voting manually (to avoid automatic timing setup)
    {
        let mut game = state.game.write().await;
        if let Some(g) = game.as_mut() {
            g.phase = GamePhase::Voting;
        }
    }

    // Generate a valid vote challenge (required for vote validation)
    state.generate_vote_challenge().await;
    let challenge_nonce = state
        .get_vote_challenge_nonce()
        .await
        .expect("Should have challenge nonce after generating");

    // Immediately submit a vote (should be within 500ms and shadow-rejected)
    let player_sub = submissions
        .iter()
        .find(|s| s.author_kind == gptdash::types::AuthorKind::Player)
        .expect("Should have player submission");

    let vote_result = handle_message(
        ClientMessage::Vote {
            voter_token: "early_voter".to_string(),
            ai: player_sub.id.clone(),
            funny: player_sub.id.clone(),
            msg_id: "early_vote".to_string(),
            challenge_nonce: challenge_nonce.clone(),
            challenge_response: compute_challenge_response(&challenge_nonce, "early_voter"),
            is_webdriver: false,
        },
        &audience_role,
        &state,
    )
    .await;

    // VoteAck should be returned (shadow rejection returns ack to not reveal detection)
    match vote_result {
        Some(ServerMessage::VoteAck { msg_id }) => {
            assert_eq!(msg_id, "early_vote");
        }
        _ => panic!("Expected VoteAck message"),
    }

    // But vote should NOT be stored (shadow rejection)
    let votes = state.votes.read().await;
    assert_eq!(
        votes.len(),
        0,
        "Vote within 500ms should be shadow-rejected and not stored"
    );

    println!("✅ Vote timing early rejection test passed!");
}

/// Test that votes submitted after 500ms are accepted
#[tokio::test]
async fn test_vote_timing_after_window() {
    // Ensure anti-automation is NOT disabled
    std::env::remove_var("SKIP_VOTE_ANTI_AUTOMATION");

    let state = Arc::new(AppState::new());
    let host_role = Role::Host;
    let audience_role = Role::Audience;

    state.create_game().await;

    // Create audience member (required for voter validation)
    state.get_or_create_audience_member("late_voter").await;

    // Create player and setup round
    let create_result = handle_message(
        ClientMessage::HostCreatePlayers { count: 1 },
        &host_role,
        &state,
    )
    .await;

    let player_tokens = match create_result {
        Some(ServerMessage::PlayersCreated { players }) => players,
        _ => panic!("Expected PlayersCreated"),
    };

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
            Some("Vote timing test prompt 2".to_string()),
            None,
            PromptSource::Host,
            None,
        )
        .await
        .unwrap();
    state
        .select_prompt(&round.id, &prompt.id, None)
        .await
        .unwrap();

    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Writing,
        },
        &host_role,
        &state,
    )
    .await;

    // Player submits
    handle_message(
        ClientMessage::SubmitAnswer {
            player_token: Some(player_tokens[0].token.clone()),
            text: "Player answer for timing test 2".to_string(),
        },
        &Role::Player,
        &state,
    )
    .await;

    // Add AI submission and set reveal order
    let ai_sub = state
        .submit_answer(&round.id, None, "AI answer 2".to_string())
        .await
        .unwrap();
    let submissions = state.get_submissions(&round.id).await;
    let reveal_order: Vec<_> = submissions.iter().map(|s| s.id.clone()).collect();
    state
        .set_reveal_order(&round.id, reveal_order)
        .await
        .unwrap();
    state
        .set_ai_submission(&round.id, ai_sub.id.clone())
        .await
        .unwrap();

    // Transition through Reveal to Voting
    handle_message(
        ClientMessage::HostTransitionPhase {
            phase: GamePhase::Reveal,
        },
        &host_role,
        &state,
    )
    .await;

    // Manually set voting phase start to 600ms ago (outside the 500ms window)
    let past_time = chrono::Utc::now() - chrono::Duration::milliseconds(600);
    *state.voting_phase_started_at.write().await = Some(past_time);

    // Set game phase to Voting manually
    {
        let mut game = state.game.write().await;
        if let Some(g) = game.as_mut() {
            g.phase = GamePhase::Voting;
        }
    }

    // Generate a valid challenge nonce
    state.generate_vote_challenge().await;
    let challenge_nonce = state
        .get_vote_challenge_nonce()
        .await
        .expect("Should have challenge nonce");

    // Submit a vote (should be accepted - outside 500ms window)
    let player_sub = submissions
        .iter()
        .find(|s| s.author_kind == gptdash::types::AuthorKind::Player)
        .expect("Should have player submission");

    let vote_result = handle_message(
        ClientMessage::Vote {
            voter_token: "late_voter".to_string(),
            ai: player_sub.id.clone(),
            funny: player_sub.id.clone(),
            msg_id: "late_vote".to_string(),
            challenge_nonce: challenge_nonce.clone(),
            challenge_response: compute_challenge_response(&challenge_nonce, "late_voter"),
            is_webdriver: false,
        },
        &audience_role,
        &state,
    )
    .await;

    // VoteAck should be returned
    match vote_result {
        Some(ServerMessage::VoteAck { msg_id }) => {
            assert_eq!(msg_id, "late_vote");
        }
        _ => panic!("Expected VoteAck message"),
    }

    // Vote SHOULD be stored (outside the 500ms window)
    let votes = state.votes.read().await;
    assert_eq!(
        votes.len(),
        1,
        "Vote after 500ms should be accepted and stored"
    );

    println!("✅ Vote timing after window test passed!");
}

// ============================================================================
// Audience Prompt Submission Tests
// ============================================================================

/// Test fuzzy deduplication of similar prompts from audience
#[tokio::test]
async fn test_audience_prompt_fuzzy_deduplication() {
    let state = Arc::new(AppState::new());

    state.create_game().await;

    // Submit first prompt
    let result1 = state
        .add_prompt_to_pool(
            Some("What is the meaning of life?".to_string()),
            None,
            PromptSource::Audience,
            Some("voter1".to_string()),
        )
        .await;
    assert!(result1.is_ok(), "First prompt should succeed");

    // Submit a very similar prompt (slightly different wording/case)
    // Fuzzy deduplication should merge them
    let result2 = state
        .add_prompt_to_pool(
            Some("what is meaning of life".to_string()),
            None,
            PromptSource::Audience,
            Some("voter2".to_string()),
        )
        .await;
    assert!(result2.is_ok(), "Second similar prompt should succeed");

    // Verify only 1 prompt exists (fuzzy dedup merged them)
    let pool = state.prompt_pool.read().await;
    assert_eq!(
        pool.len(),
        1,
        "Similar prompts should be merged into one (fuzzy dedup)"
    );

    let merged_prompt = pool.values().next().unwrap();

    // Verify submission_count was incremented
    assert_eq!(
        merged_prompt.submission_count, 2,
        "Merged prompt should have submission_count of 2"
    );

    // Verify both submitter_ids are recorded
    assert!(
        merged_prompt.submitter_ids.contains(&"voter1".to_string()),
        "First submitter should be recorded"
    );
    assert!(
        merged_prompt.submitter_ids.contains(&"voter2".to_string()),
        "Second submitter should be recorded"
    );

    println!("✅ Audience prompt fuzzy deduplication test passed!");
}
