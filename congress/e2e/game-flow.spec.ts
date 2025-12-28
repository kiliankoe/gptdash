import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  getText,
  waitForBeamerScene,
  getPlayerTokens,
  resetGameState,
  createGameClients,
  closeContexts,
  debugLog,
} from "./test-utils";

/**
 * Full game flow integration test
 *
 * Tests the complete GPTDash game flow from lobby to results with:
 * - 1 Host
 * - 1 Beamer
 * - 2 Players
 * - 2 Audience members
 */
test.describe("Game Flow", () => {
  let contexts: BrowserContext[] = [];
  let clients: GameClients;

  test.beforeEach(async ({ browser }) => {
    const result = await createGameClients(browser);
    clients = result.clients;
    contexts = result.contexts;
    await resetGameState(browser);
  });

  test.afterEach(async () => {
    await closeContexts(contexts);
  });

  test("complete game from lobby to results with multiple players and audience", async ({
    browser,
  }) => {
    // Extended test with 27 steps covering 2 rounds + game reset + new game with new players
    test.setTimeout(120000); // 120 seconds (2 minutes)

    const { host, beamer, players, audience } = clients;

    // ============================================
    // STEP 1: Connect all clients
    // ============================================
    debugLog("Step 1: Connecting all clients...");

    // Navigate to pages in parallel
    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer"),
      players[0].goto("/player"),
      players[1].goto("/player"),
      audience[0].goto("/"),
      audience[1].goto("/"),
    ]);

    // Wait for all connections
    await Promise.all([
      waitForConnection(host),
      waitForConnection(beamer),
      // Players and audience don't have the same status indicator initially
    ]);

    // Verify beamer shows lobby scene
    await waitForBeamerScene(beamer, "sceneLobby");

    // Verify host shows LOBBY phase
    await expect(host.locator("#overviewPhase")).toHaveText("LOBBY");

    // ============================================
    // STEP 2: Host creates player tokens
    // ============================================
    debugLog("Step 2: Creating player tokens...");

    // Navigate to Players panel
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Set player count to 2
    await host.fill("#playerCount", "2");
    await host.click('#players button:has-text("Spieler erstellen")');

    // Wait for tokens to appear
    await host.waitForSelector("#playerTokensList .token");
    const tokens = await getPlayerTokens(host);
    expect(tokens).toHaveLength(2);
    expect(tokens[0]).toMatch(/^[A-Z0-9]+$/);
    expect(tokens[1]).toMatch(/^[A-Z0-9]+$/);

    debugLog(`Created tokens: ${tokens.join(", ")}`);

    // ============================================
    // STEP 3: Players join with tokens
    // ============================================
    debugLog("Step 3: Players joining...");

    // Player 1 joins
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");

    // Player 1 registers name
    await players[0].fill("#nameInput", "Alice");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Player 2 joins
    await players[1].fill("#tokenInput", tokens[1]);
    await players[1].click("#joinButton");
    await players[1].waitForSelector("#registerScreen.active");

    // Player 2 registers name
    await players[1].fill("#nameInput", "Bob");
    await players[1].click("#registerButton");
    await players[1].waitForSelector("#waitingScreen.active");

    // ============================================
    // STEP 4: Audience members join
    // ============================================
    debugLog("Step 4: Audience joining...");

    // Audience 1 joins (may auto-advance to waiting screen after WS connect)
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active");

    // Audience 2 joins (may auto-advance to waiting screen after WS connect)
    if (await audience[1].locator("#joinButton").isVisible()) {
      await audience[1].click("#joinButton");
    }
    await audience[1].waitForSelector("#waitingScreen.active");

    // ============================================
    // STEP 5: Host adds and queues prompt
    // ============================================
    debugLog("Step 5: Adding and queueing prompt...");

    // Navigate to prompts panel
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");

    // Add a prompt to the pool
    await host.fill(
      "#promptText",
      "What is the meaning of life, the universe, and everything?",
    );
    await host.click('#prompts button:has-text("Prompt hinzufügen")');

    // Wait for prompt to appear in the pool list (compact rows)
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");

    // Queue the prompt (new flow: queue -> start -> auto-advance to WRITING)
    await host.locator("#hostPromptsList .queue-btn").first().click();

    // Wait for start button to become visible (triggered by server response)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // ============================================
    // STEP 6: Host starts prompt selection (auto-advances to WRITING with 1 prompt)
    // ============================================
    debugLog(
      "Step 6: Starting prompt selection (will auto-advance to WRITING)...",
    );

    // Click the start button to begin prompt selection
    await host.click("#startPromptSelectionBtn");
    await host.waitForTimeout(1000);

    // Wait for phase change - should auto-advance to WRITING since only 1 prompt
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Verify beamer shows writing scene
    await waitForBeamerScene(beamer, "sceneWriting");

    // ============================================
    // STEP 7: Players submit answers
    // ============================================
    debugLog("Step 7: Players submitting answers...");

    // Wait for players to see writing screen
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[1].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // Player 1 submits answer
    await players[0].fill(
      "#answerInput",
      "The answer is 42, as computed by Deep Thought over millions of years. This is the ultimate answer to everything.",
    );
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Player 2 submits answer
    await players[1].fill(
      "#answerInput",
      "Life has no inherent meaning - we create our own purpose through our choices and connections with others.",
    );
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // Verify submissions appear in host view
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });

    const submissionCount = await host.locator(".submission-card").count();
    expect(submissionCount).toBeGreaterThanOrEqual(2);

    // Ensure there's an AI submission selected (required for RESULTS phase).
    // Use the host's manual AI override so tests don't depend on external LLMs.
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill(
      "#manualAiText",
      "Manual AI answer for e2e tests (deterministic).",
    );
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // ============================================
    // STEP 8: Host transitions to REVEAL
    // ============================================
    debugLog("Step 8: Transitioning to reveal...");

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');

    await expect(host.locator("#overviewPhase")).toHaveText("REVEAL", {
      timeout: 5000,
    });
    await waitForBeamerScene(beamer, "sceneReveal");

    // Players should see locked screen
    await players[0].waitForSelector("#lockedScreen.active", { timeout: 5000 });
    await players[1].waitForSelector("#lockedScreen.active", { timeout: 5000 });

    // ============================================
    // STEP 9: Host navigates through reveals
    // ============================================
    debugLog("Step 9: Navigating reveal carousel...");

    await host.click('.sidebar-item:has-text("Antworten")');

    // Click next to reveal first answer
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // Verify beamer shows reveal card
    const revealText = await getText(beamer, "#revealText");
    expect(revealText.length).toBeGreaterThan(10);

    // Navigate to next answer
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // ============================================
    // STEP 10: Host transitions to VOTING
    // ============================================
    debugLog("Step 10: Transitioning to voting...");

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');

    await expect(host.locator("#overviewPhase")).toHaveText("VOTING", {
      timeout: 5000,
    });
    await waitForBeamerScene(beamer, "sceneVoting");

    // Late-joining audience member should immediately see voting options
    const lateAudienceContext = await browser.newContext();
    contexts.push(lateAudienceContext);
    const lateAudience = await lateAudienceContext.newPage();
    await lateAudience.goto("/");
    await lateAudience.waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await lateAudience.waitForSelector("#votingScreen.active .answer-option", {
      timeout: 5000,
    });

    // Audience should see voting screen
    await audience[0].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await audience[1].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });

    // ============================================
    // STEP 11: Audience votes
    // ============================================
    debugLog("Step 11: Audience voting...");

    // Wait for answer options to appear
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });
    await audience[1].waitForSelector(".answer-option", { timeout: 5000 });

    // Audience 1 votes
    const aiOptions1 = audience[0].locator("#aiAnswerOptions .answer-option");
    const funnyOptions1 = audience[0].locator(
      "#funnyAnswerOptions .answer-option",
    );

    // Click first answer for AI, second for funny
    await aiOptions1.first().click();
    await funnyOptions1.last().click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    // Audience 2 votes
    const aiOptions2 = audience[1].locator("#aiAnswerOptions .answer-option");
    const funnyOptions2 = audience[1].locator(
      "#funnyAnswerOptions .answer-option",
    );

    // Click second answer for AI, first for funny (different from audience 1)
    await aiOptions2.last().click();
    await funnyOptions2.first().click();
    await audience[1].click("#voteButton");
    await audience[1].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    // Wait for vote counts to propagate to beamer
    await beamer.waitForTimeout(1000);

    // Verify vote bars are showing on beamer
    const voteBars = beamer.locator(".vote-bar");
    const voteBarCount = await voteBars.count();
    expect(voteBarCount).toBeGreaterThan(0);

    // ============================================
    // STEP 12: Host transitions to RESULTS
    // ============================================
    debugLog("Step 12: Transitioning to results...");

    await host.click('button[data-phase="RESULTS"]');

    await expect(host.locator("#overviewPhase")).toHaveText("RESULTS", {
      timeout: 5000,
    });
    await waitForBeamerScene(beamer, "sceneResultsBreakdown");

    // Audience should see results screen
    await audience[0].waitForSelector("#resultsScreen.active", {
      timeout: 5000,
    });
    await audience[1].waitForSelector("#resultsScreen.active", {
      timeout: 5000,
    });

    // Verify breakdown view shows vote counts
    const breakdownGrid = beamer.locator("#breakdownGrid");
    await expect(breakdownGrid).toBeVisible();

    // Advance to leaderboards step
    await host.click('.sidebar-item:has-text("bersicht")');
    await host.click("#overviewPrimaryActionBtn"); // "Leaderboards zeigen"
    await waitForBeamerScene(beamer, "sceneResultsLeaderboards");

    // ============================================
    // STEP 13: Verify scores are displayed
    // ============================================
    debugLog("Step 13: Verifying scores...");

    // Check host scores panel
    await host.click('.sidebar-item:has-text("Punkte")');
    await host.waitForSelector("#scores.active");

    // There should be some score information
    // (exact values depend on which submission was marked as AI)
    const playerScoresSection = host.locator("#playerScores");
    await expect(playerScoresSection).toBeVisible();

    // Beamer should show leaderboard
    const leaderboard = beamer.locator("#leaderboardList");
    await expect(leaderboard).toBeVisible();

    // ============================================
    // STEP 14: Host transitions to PODIUM and audience winner screen
    // ============================================
    debugLog("Step 14: Transitioning to podium and checking winner screens...");

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('.sidebar-item:has-text("bersicht")');
    await host.click("#overviewPrimaryActionBtn"); // "Podium"

    await expect(host.locator("#overviewPhase")).toHaveText("PODIUM", {
      timeout: 5000,
    });
    await waitForBeamerScene(beamer, "scenePodium");

    // Give time for scores message to propagate
    await host.waitForTimeout(500);

    // At least one audience member should see either:
    // - winnerFullscreen (if they correctly identified AI and are in top 3)
    // - waitingScreen (if they didn't identify AI correctly)
    // Check that both audience members have appropriate screens
    const audience0Screen = await audience[0]
      .locator("#winnerFullscreen.active, #waitingScreen.active")
      .first()
      .getAttribute("id");
    const audience1Screen = await audience[1]
      .locator("#winnerFullscreen.active, #waitingScreen.active")
      .first()
      .getAttribute("id");

    debugLog(`Audience 0 sees: ${audience0Screen}`);
    debugLog(`Audience 1 sees: ${audience1Screen}`);

    // Both should have valid screens (either winner or waiting)
    expect(["winnerFullscreen", "waitingScreen"]).toContain(audience0Screen);
    expect(["winnerFullscreen", "waitingScreen"]).toContain(audience1Screen);

    // If winner screen is shown, verify the green pulsing animation is applied
    const winnerScreen = audience[0].locator("#winnerFullscreen.active");
    if ((await winnerScreen.count()) > 0) {
      // Verify it has the pulse animation
      const animationName = await winnerScreen.evaluate(
        (el) => getComputedStyle(el).animationName,
      );
      expect(animationName).toBe("winnerPulse");
      debugLog("Winner screen verified with pulsing animation!");
    }

    // ============================================
    // STEP 15: Start a second round and verify UI resets
    // ============================================
    debugLog("Step 15: Starting second round and verifying state reset...");

    // Add a second prompt to the pool
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill(
      "#promptText",
      "Round 2: Describe a futuristic parliament.",
    );
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");

    // Queue and start (single prompt -> auto-advance to WRITING)
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });
    await host.click("#startPromptSelectionBtn");

    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });
    await expect(host.locator("#overviewRound")).toHaveText("2", {
      timeout: 5000,
    });
    await waitForBeamerScene(beamer, "sceneWriting");

    // Host submissions should be cleared for the new round
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await expect(host.locator(".submission-card")).toHaveCount(0);

    // Players should see WRITING with the new prompt and an empty textbox (no prefill)
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[1].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await expect(players[0].locator("#answerInput")).toHaveValue("");
    await expect(players[1].locator("#answerInput")).toHaveValue("");
    await expect(players[0].locator("#promptText")).toHaveText(
      "Round 2: Describe a futuristic parliament.",
    );
    await expect(players[1].locator("#promptText")).toHaveText(
      "Round 2: Describe a futuristic parliament.",
    );

    // Beamer should show the new prompt
    await expect(beamer.locator("#writingPromptText")).toHaveText(
      "Round 2: Describe a futuristic parliament.",
    );

    // ============================================
    // STEP 16: Players submit answers for Round 2
    // ============================================
    debugLog("Step 16: Players submitting round 2 answers...");

    // Player 1 submits round 2 answer
    await players[0].fill(
      "#answerInput",
      "A holographic assembly where AI representatives debate alongside humans in a transparent dome.",
    );
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Player 2 submits round 2 answer
    await players[1].fill(
      "#answerInput",
      "Parliament meets in virtual reality, with instant translation and emotion sensing for all members.",
    );
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // ============================================
    // STEP 17: Set AI answer and transition through Reveal
    // ============================================
    debugLog("Step 17: Setting AI answer and transitioning through reveal...");

    // Set manual AI answer for round 2
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });
    // Open the manual AI answer details section - click and ensure it expands
    const summaryElement = host.locator(
      'summary:has-text("Manuelle KI-Antwort")',
    );
    await summaryElement.click();
    await host.waitForTimeout(300);
    // If still not visible, try clicking again (details might need re-toggle)
    if (!(await host.locator("#manualAiText").isVisible())) {
      await summaryElement.click();
      await host.waitForTimeout(300);
    }
    await host.waitForSelector("#manualAiText", {
      state: "visible",
      timeout: 5000,
    });
    await host.fill(
      "#manualAiText",
      "Round 2 AI answer: Quantum-networked telepresence pods allow global participation.",
    );
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // Transition to REVEAL
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await expect(host.locator("#overviewPhase")).toHaveText("REVEAL", {
      timeout: 5000,
    });

    // Navigate through reveals
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // ============================================
    // STEP 18: Audience votes and transition to Results/Podium
    // ============================================
    debugLog("Step 18: Audience voting for round 2...");

    // Transition to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("VOTING", {
      timeout: 5000,
    });

    // Audience votes (reusing existing audience pages - no refresh)
    await audience[0].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });
    await audience[0]
      .locator("#aiAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0]
      .locator("#funnyAnswerOptions .answer-option")
      .last()
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    await audience[1].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await audience[1].waitForSelector(".answer-option", { timeout: 5000 });
    await audience[1].locator("#aiAnswerOptions .answer-option").last().click();
    await audience[1]
      .locator("#funnyAnswerOptions .answer-option")
      .first()
      .click();
    await audience[1].click("#voteButton");
    await audience[1].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    // Transition to RESULTS then PODIUM
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="RESULTS"]');
    await expect(host.locator("#overviewPhase")).toHaveText("RESULTS", {
      timeout: 5000,
    });

    await host.click('button[data-phase="PODIUM"]');
    await expect(host.locator("#overviewPhase")).toHaveText("PODIUM", {
      timeout: 5000,
    });

    // ============================================
    // STEP 19: Verify final scores before reset
    // ============================================
    debugLog("Step 19: Verifying final scores before reset...");

    // Navigate to scores panel
    await host.click('.sidebar-item:has-text("Punkte")');
    await host.waitForSelector("#scores.active");

    // Store audience display names for later comparison (shown in header)
    const audience0Name =
      (await audience[0].locator("#headerDisplayName").textContent()) || "";
    const audience1Name =
      (await audience[1].locator("#headerDisplayName").textContent()) || "";
    debugLog(`Audience names: ${audience0Name}, ${audience1Name}`);

    // Verify player scores exist (they played 2 rounds)
    await expect(host.locator("#playerScores")).toBeVisible();

    // ============================================
    // STEP 20: Reset game for new players
    // ============================================
    debugLog("Step 20: Resetting game for new players...");

    // Accept confirmation dialog
    host.on("dialog", (dialog) => dialog.accept());

    // Navigate to game control and reset
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");
    await host.click('button:has-text("Spiel zurücksetzen")');
    await host.waitForTimeout(500);

    // Verify phase returns to LOBBY
    await expect(host.locator("#overviewPhase")).toHaveText("LOBBY", {
      timeout: 5000,
    });
    await waitForBeamerScene(beamer, "sceneLobby");

    // Verify round counter reset
    await expect(host.locator("#overviewRound")).toHaveText("0");

    // ============================================
    // STEP 21: Verify old players see token entry screen
    // ============================================
    debugLog("Step 21: Verifying old players see token entry screen...");

    // After reset, old player pages will show waitingScreen (stale client state).
    // To trigger token validation, we reload the pages - this causes reconnection
    // with the now-invalid token, triggering INVALID_PLAYER_TOKEN error → joinScreen
    await players[0].reload();
    await players[1].reload();

    // Old player pages should now show token entry (their tokens are invalid on server)
    await players[0].waitForSelector("#joinScreen.active", { timeout: 5000 });
    await players[1].waitForSelector("#joinScreen.active", { timeout: 5000 });

    // Verify token input is visible
    await expect(players[0].locator("#tokenInput")).toBeVisible();
    await expect(players[1].locator("#tokenInput")).toBeVisible();

    // Verify error message about invalid token is shown
    await expect(players[0].locator("#joinError")).toContainText(
      "Ungültiger Token",
    );
    await expect(players[1].locator("#joinError")).toContainText(
      "Ungültiger Token",
    );

    // ============================================
    // STEP 22: Verify prompts still available in pool
    // ============================================
    debugLog("Step 22: Verifying prompts functionality after reset...");

    // Navigate to Prompts panel
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");

    // Add a fresh prompt for the new game
    await host.fill(
      "#promptText",
      "New game prompt: What would a robot's vacation look like?",
    );
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");

    // ============================================
    // STEP 23: Verify audience still connected (no refresh needed)
    // ============================================
    debugLog("Step 23: Verifying audience connections survived reset...");

    // Audience should still be on waiting screen (their connections survived the reset)
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });
    await audience[1].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // Their display names should persist (shown in header)
    await expect(audience[0].locator("#headerDisplayName")).toHaveText(
      audience0Name,
    );
    await expect(audience[1].locator("#headerDisplayName")).toHaveText(
      audience1Name,
    );

    // ============================================
    // STEP 24: Test audience page refresh and reconnection
    // ============================================
    debugLog("Step 24: Testing audience page refresh and reconnection...");

    // One audience member refreshes their page to test reconnection
    await audience[0].reload();
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // After reconnection, same display name should be preserved
    await expect(audience[0].locator("#headerDisplayName")).toHaveText(
      audience0Name,
    );

    // ============================================
    // STEP 25: Create new players and start new game
    // ============================================
    debugLog("Step 25: Creating new players for new game...");

    // Create new player tokens
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "2");
    await host.click('#players button:has-text("Spieler erstellen")');
    // Wait for tokens to appear AND be fully rendered (not just the container)
    await host.waitForSelector("#playerTokensList .player-status-card");
    // Additional wait to ensure server state is synchronized
    await host.waitForTimeout(500);
    const newTokens = await getPlayerTokens(host);
    debugLog(`New tokens: ${newTokens.join(", ")}`);
    // Verify we got valid tokens
    expect(newTokens.length).toBe(2);
    expect(newTokens[0]).toMatch(/^[A-Z0-9]{5}$/);
    expect(newTokens[1]).toMatch(/^[A-Z0-9]{5}$/);

    // Verify new tokens were created (2 new tokens for the new game)
    expect(newTokens.length).toBe(2);
    expect(newTokens[0].length).toBeGreaterThan(0);
    expect(newTokens[1].length).toBeGreaterThan(0);

    // Verify new tokens show in host UI
    const tokenCards = await host.locator("#playerTokensList .player-token");
    await expect(tokenCards).toHaveCount(2);

    // Create fresh browser contexts for new players
    const charlieContext = await browser.newContext();
    const dianaContext = await browser.newContext();
    contexts.push(charlieContext, dianaContext);
    const charlie = await charlieContext.newPage();
    const diana = await dianaContext.newPage();

    // Charlie joins with new token
    await charlie.goto("/player");
    await charlie.waitForSelector("#tokenInput", { state: "visible" });
    await charlie.fill("#tokenInput", newTokens[0]);
    await charlie.click("#joinButton");
    await charlie.waitForSelector("#registerScreen.active");
    // Wait for any animations to settle before interacting
    await charlie.waitForTimeout(500);
    await charlie.waitForSelector("#registerButton", { state: "visible" });
    await charlie.fill("#nameInput", "Charlie");
    await charlie.waitForSelector("#registerButton", { state: "visible" });
    await charlie.click("#registerButton");
    await charlie.waitForSelector("#waitingScreen.active");

    // Diana joins with new token
    await diana.goto("/player");
    await diana.waitForSelector("#tokenInput", { state: "visible" });
    await diana.fill("#tokenInput", newTokens[1]);
    await diana.click("#joinButton");
    await diana.waitForSelector("#registerScreen.active");
    // Wait for any animations to settle before interacting
    await diana.waitForTimeout(500);
    await diana.waitForSelector("#registerButton", { state: "visible" });
    await diana.fill("#nameInput", "Diana");
    await diana.waitForSelector("#registerButton", { state: "visible" });
    await diana.click("#registerButton");
    await diana.waitForSelector("#waitingScreen.active");

    // Verify old players (Alice/Bob) are still on joinScreen from step 21
    await expect(players[0].locator("#joinScreen")).toBeVisible();
    await expect(players[1].locator("#joinScreen")).toBeVisible();

    // ============================================
    // STEP 26: Run new game round through voting
    // ============================================
    debugLog("Step 26: Running new game round through voting...");

    // Queue and start prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
    });
    await host.click("#startPromptSelectionBtn");
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING");
    await expect(host.locator("#overviewRound")).toHaveText("1"); // Back to round 1!

    // New players submit answers
    await charlie.waitForSelector("#writingScreen.active");
    await charlie.fill(
      "#answerInput",
      "Robots vacation at server farms, defragmenting their memories.",
    );
    await charlie.click("#submitButton");
    await charlie.waitForSelector("#submittedScreen.active");

    await diana.waitForSelector("#writingScreen.active");
    await diana.fill(
      "#answerInput",
      "A robot vacation involves visiting antique technology museums.",
    );
    await diana.click("#submitButton");
    await diana.waitForSelector("#submittedScreen.active");

    // Set AI answer
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });
    const summaryElement2 = host.locator(
      'summary:has-text("Manuelle KI-Antwort")',
    );
    await summaryElement2.click();
    await host.waitForTimeout(300);
    // If still not visible, try clicking again (details might need re-toggle)
    if (!(await host.locator("#manualAiText").isVisible())) {
      await summaryElement2.click();
      await host.waitForTimeout(300);
    }
    await host.waitForSelector("#manualAiText", {
      state: "visible",
      timeout: 5000,
    });
    await host.fill(
      "#manualAiText",
      "New game AI: Solar panel sunbathing and WiFi meditation retreats.",
    );
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card");
    await host.locator(".ai-submission-card").first().click();

    // Transition through REVEAL to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await expect(host.locator("#overviewPhase")).toHaveText("REVEAL");

    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.click('#submissions button:has-text("Weiter")');

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("VOTING");

    // Audience votes again
    await audience[0].waitForSelector("#votingScreen.active");
    await audience[0]
      .locator("#aiAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0]
      .locator("#funnyAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active");

    await audience[1].waitForSelector("#votingScreen.active");
    await audience[1]
      .locator("#aiAnswerOptions .answer-option")
      .first()
      .click();
    await audience[1]
      .locator("#funnyAnswerOptions .answer-option")
      .first()
      .click();
    await audience[1].click("#voteButton");
    await audience[1].waitForSelector("#confirmedScreen.active");

    // ============================================
    // STEP 27: Verify new game scores are reset
    // ============================================
    debugLog("Step 27: Verifying new game scores are reset...");

    // Transition to RESULTS
    await host.click('button[data-phase="RESULTS"]');
    await expect(host.locator("#overviewPhase")).toHaveText("RESULTS");

    // Navigate to scores panel
    await host.click('.sidebar-item:has-text("Punkte")');
    await host.waitForSelector("#scores.active");

    // Verify new players appear in scores, old players don't
    const playerScoresText =
      (await host.locator("#playerScores").textContent()) || "";
    expect(playerScoresText).not.toContain("Alice");
    expect(playerScoresText).not.toContain("Bob");
    expect(playerScoresText).toContain("Charlie");
    expect(playerScoresText).toContain("Diana");

    // Verify audience scores exist
    await expect(host.locator("#audienceScores")).toBeVisible();

    debugLog(
      "Multi-game flow completed! Verified: round 2, game reset, audience persistence, " +
        "new player registration, new game round, and score isolation.",
    );
  });
});
