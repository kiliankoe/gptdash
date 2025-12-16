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
    const { host, beamer, players, audience } = clients;

    // ============================================
    // STEP 1: Connect all clients
    // ============================================
    console.log("Step 1: Connecting all clients...");

    // Navigate to pages in parallel
    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer.html"),
      players[0].goto("/player.html"),
      players[1].goto("/player.html"),
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
    console.log("Step 2: Creating player tokens...");

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

    console.log(`Created tokens: ${tokens.join(", ")}`);

    // ============================================
    // STEP 3: Players join with tokens
    // ============================================
    console.log("Step 3: Players joining...");

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
    console.log("Step 4: Audience joining...");

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
    console.log("Step 5: Adding and queueing prompt...");

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
    console.log(
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
    console.log("Step 7: Players submitting answers...");

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
    console.log("Step 8: Transitioning to reveal...");

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
    console.log("Step 9: Navigating reveal carousel...");

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
    console.log("Step 10: Transitioning to voting...");

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
    console.log("Step 11: Audience voting...");

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
    console.log("Step 12: Transitioning to results...");

    await host.click('button[data-phase="RESULTS"]');

    await expect(host.locator("#overviewPhase")).toHaveText("RESULTS", {
      timeout: 5000,
    });
    await waitForBeamerScene(beamer, "sceneResults");

    // Audience should see results screen
    await audience[0].waitForSelector("#resultsScreen.active", {
      timeout: 5000,
    });
    await audience[1].waitForSelector("#resultsScreen.active", {
      timeout: 5000,
    });

    // ============================================
    // STEP 13: Verify scores are displayed
    // ============================================
    console.log("Step 13: Verifying scores...");

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
    console.log("Step 14: Transitioning to podium and checking winner screens...");

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="PODIUM"]');

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

    console.log(`Audience 0 sees: ${audience0Screen}`);
    console.log(`Audience 1 sees: ${audience1Screen}`);

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
      console.log("Winner screen verified with pulsing animation!");
    }

    // ============================================
    // STEP 15: Start a second round and verify UI resets
    // ============================================
    console.log("Step 15: Starting second round and verifying state reset...");

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

    console.log(
      "Full game flow (including PODIUM winner screen and round 2 start) completed successfully!",
    );
  });
});
