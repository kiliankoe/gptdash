use gptdash::protocol::{ClientMessage, ServerMessage};
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

    match phase_result {
        Some(ServerMessage::GameState { game, .. }) => {
            assert_eq!(game.phase, GamePhase::PromptSelection);
        }
        _ => panic!("Expected GameState message"),
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
        .add_prompt(
            &round.id,
            "What is the meaning of life?".to_string(),
            PromptSource::Host,
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

    match intermission_result {
        Some(ServerMessage::GameState { game, .. }) => {
            assert_eq!(
                game.phase,
                GamePhase::Intermission,
                "Should transition to Intermission"
            );
        }
        _ => panic!("Expected GameState message for Intermission transition"),
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
        Some(ServerMessage::GameState { game, .. }) => {
            assert_eq!(
                game.phase,
                GamePhase::Results,
                "Should transition back to Results"
            );
        }
        _ => panic!("Expected GameState message for Results re-entry"),
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
