import { test, expect, Page, BrowserContext } from "@playwright/test";

/**
 * Full game flow end-to-end test
 *
 * Tests the complete GPTDash game flow with:
 * - 1 Host
 * - 1 Beamer
 * - 2 Players
 * - 2 Audience members
 */

interface GameClients {
  host: Page;
  beamer: Page;
  players: Page[];
  audience: Page[];
}

// Helper to wait for WebSocket connection
async function waitForConnection(page: Page, timeout = 10000): Promise<void> {
  await page.waitForFunction(
    () => {
      const dot = document.getElementById("statusDot");
      return dot?.classList.contains("connected");
    },
    { timeout }
  );
}

// Helper to get text content safely
async function getText(page: Page, selector: string): Promise<string> {
  const element = await page.$(selector);
  return element ? ((await element.textContent()) ?? "") : "";
}

// Helper to wait for phase on beamer
async function waitForBeamerScene(
  beamer: Page,
  sceneId: string,
  timeout = 10000
): Promise<void> {
  await beamer.waitForSelector(`#${sceneId}.active`, { timeout });
}

// Helper to extract player tokens from host UI
async function getPlayerTokens(host: Page): Promise<string[]> {
  return host.$$eval("#playerTokensList .token", (els) =>
    els.map((el) => el.textContent?.trim() ?? "")
  );
}

test.describe("Full Game Flow", () => {
  let contexts: BrowserContext[] = [];
  let clients: GameClients;

  test.beforeEach(async ({ browser }) => {
    // Create isolated contexts for each role
    const hostContext = await browser.newContext();
    const beamerContext = await browser.newContext();
    const player1Context = await browser.newContext();
    const player2Context = await browser.newContext();
    const audience1Context = await browser.newContext();
    const audience2Context = await browser.newContext();

    contexts = [
      hostContext,
      beamerContext,
      player1Context,
      player2Context,
      audience1Context,
      audience2Context,
    ];

    // Create pages
    const hostPage = await hostContext.newPage();

    // Capture console logs from host page for debugging
    hostPage.on("console", (msg) => {
      if (
        msg.text().includes("host_submissions") ||
        msg.text().includes("Unhandled") ||
        msg.text().includes("updateSubmissionsList") ||
        msg.text().includes("submissionsList") ||
        msg.text().includes("game_state") ||
        msg.text().includes("Rendering") ||
        msg.text().includes("Creating card")
      ) {
        console.log(`[HOST CONSOLE] ${msg.type()}: ${msg.text()}`);
      }
    });

    clients = {
      host: hostPage,
      beamer: await beamerContext.newPage(),
      players: [await player1Context.newPage(), await player2Context.newPage()],
      audience: [
        await audience1Context.newPage(),
        await audience2Context.newPage(),
      ],
    };

    // Reset game state before each test using a separate context
    const resetContext = await browser.newContext();
    const resetPage = await resetContext.newPage();
    await resetPage.goto("/host.html");
    await waitForConnection(resetPage);

    // Handle reset confirmation dialog
    resetPage.on("dialog", (dialog) => dialog.accept());
    await resetPage.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await resetPage.waitForSelector("#game.active");
    await resetPage.click('button:has-text("Spiel zurücksetzen")');
    await resetPage.waitForTimeout(500);
    await resetContext.close();
  });

  test.afterEach(async () => {
    // Close all contexts
    for (const ctx of contexts) {
      await ctx.close();
    }
  });

  test("complete game from lobby to results with multiple players and audience", async () => {
    const { host, beamer, players, audience } = clients;

    // ============================================
    // STEP 1: Connect all clients
    // ============================================
    console.log("Step 1: Connecting all clients...");

    // Navigate to pages in parallel
    await Promise.all([
      host.goto("/host.html"),
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

    // Audience 1 joins
    await audience[0].click("#joinButton");
    await audience[0].waitForSelector("#waitingScreen.active");

    // Audience 2 joins
    await audience[1].click("#joinButton");
    await audience[1].waitForSelector("#waitingScreen.active");

    // ============================================
    // STEP 5: Host starts round and adds prompt
    // ============================================
    console.log("Step 5: Starting round and adding prompt...");

    // Navigate to game control
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // Start round first (required before adding prompts)
    await host.click('button:has-text("Neue Runde starten")');
    await host.waitForTimeout(500);

    // Navigate to prompts panel
    await host.click('.sidebar-item:has-text("Fragen")');
    await host.waitForSelector("#prompts.active");

    // Add a prompt (auto-selects when added by host)
    await host.fill(
      "#promptText",
      "What is the meaning of life, the universe, and everything?"
    );
    await host.click('#prompts button:has-text("Frage hinzufügen")');

    // Wait a bit for prompt to be added and selected
    await host.waitForTimeout(500);

    // ============================================
    // STEP 6: Host transitions to writing (via PROMPT_SELECTION)
    // ============================================
    console.log("Step 6: Transitioning to writing...");

    // Navigate to game control
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // First transition to PROMPT_SELECTION (required from LOBBY)
    await host.click('button[data-phase="PROMPT_SELECTION"]');
    await host.waitForTimeout(500);

    // Now transition to WRITING (prompt is already selected)
    await host.click('button[data-phase="WRITING"]');

    // Wait for phase change
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
    await players[0].waitForSelector("#writingScreen.active", { timeout: 5000 });
    await players[1].waitForSelector("#writingScreen.active", { timeout: 5000 });

    // Player 1 submits answer
    await players[0].fill(
      "#answerInput",
      "The answer is 42, as computed by Deep Thought over millions of years. This is the ultimate answer to everything."
    );
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Player 2 submits answer
    await players[1].fill(
      "#answerInput",
      "Life has no inherent meaning - we create our own purpose through our choices and connections with others."
    );
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // Verify submissions appear in host view
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });

    const submissionCount = await host.locator(".submission-card").count();
    expect(submissionCount).toBeGreaterThanOrEqual(2);

    // Mark first submission as AI (required for RESULTS phase)
    // Must be done during WRITING phase while "Als KI markieren" buttons are visible
    const aiButtonsWriting = host.locator('button:has-text("Als KI markieren")');
    const aiButtonCountWriting = await aiButtonsWriting.count();
    console.log(`Found ${aiButtonCountWriting} AI marking buttons during WRITING`);
    if (aiButtonCountWriting > 0) {
      await aiButtonsWriting.first().click();
      await host.waitForTimeout(500);
      console.log("Marked submission as AI during WRITING phase");
    }

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
    await host.click('button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // Verify beamer shows reveal card
    const revealText = await getText(beamer, "#revealText");
    expect(revealText.length).toBeGreaterThan(10);

    // Navigate to next answer
    await host.click('button:has-text("Weiter")');
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

    // Audience should see voting screen
    await audience[0].waitForSelector("#votingScreen.active", { timeout: 5000 });
    await audience[1].waitForSelector("#votingScreen.active", { timeout: 5000 });

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
      "#funnyAnswerOptions .answer-option"
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
      "#funnyAnswerOptions .answer-option"
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

    console.log("Full game flow completed successfully!");
  });

  test("player can join with token and register name", async () => {
    const { host, players } = clients;

    // Setup: Connect host, create token
    await host.goto("/host.html");
    await waitForConnection(host);

    // Navigate to Players panel
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");
    const tokens = await getPlayerTokens(host);

    // Player joins with token
    await players[0].goto("/player.html");
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");

    // Should see register screen
    await players[0].waitForSelector("#registerScreen.active", { timeout: 5000 });

    // Player registers name
    await players[0].fill("#nameInput", "TestPlayer");
    await players[0].click("#registerButton");

    // Should see waiting screen
    await players[0].waitForSelector("#waitingScreen.active", { timeout: 5000 });

    // Verify token was stored
    const storedToken = await players[0].evaluate(() =>
      localStorage.getItem("gptdash_player_token")
    );
    expect(storedToken).toBe(tokens[0]);
  });

  test("audience can join and see waiting screen", async () => {
    const { host, audience } = clients;

    // Setup minimal game - just start game
    await host.goto("/host.html");
    await waitForConnection(host);

    // Audience joins during LOBBY phase
    await audience[0].goto("/");
    await audience[0].click("#joinButton");

    // Audience should see waiting screen during LOBBY
    await audience[0].waitForSelector("#waitingScreen.active", { timeout: 5000 });

    // Verify voter token was stored
    const voterToken = await audience[0].evaluate(() =>
      localStorage.getItem("gptdash_voter_token")
    );
    expect(voterToken).toBeTruthy();
  });

  test("beamer displays correct scenes for each phase", async () => {
    const { host, beamer } = clients;

    await host.goto("/host.html");
    await beamer.goto("/beamer.html");

    await waitForConnection(host);
    await waitForConnection(beamer);

    // Initial: LOBBY
    await waitForBeamerScene(beamer, "sceneLobby");

    // Navigate to Players panel and create minimal setup for transitions
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");

    // Start round first (required before adding prompts)
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");
    await host.click('#game button:has-text("Neue Runde starten")');
    await host.waitForTimeout(500);

    // Add prompt (auto-selects when added by host)
    await host.click('.sidebar-item:has-text("Fragen")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Scene test prompt");
    await host.click('#prompts button:has-text("Frage hinzufügen")');
    await host.waitForTimeout(500);

    // Go back to game control
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // First transition to PROMPT_SELECTION (required from LOBBY)
    await host.click('button[data-phase="PROMPT_SELECTION"]');
    await host.waitForTimeout(500);

    // Now WRITING should be enabled
    await host.waitForSelector('button[data-phase="WRITING"]:not([disabled])', {
      timeout: 5000,
    });

    // WRITING
    await host.click('button[data-phase="WRITING"]');
    await waitForBeamerScene(beamer, "sceneWriting");

    // Note: REVEAL, VOTING, and RESULTS require submissions/votes
    // Testing those phases in the full game flow test instead

    // INTERMISSION (from writing)
    await host.click('button[data-phase="INTERMISSION"]');
    await waitForBeamerScene(beamer, "sceneIntermission");

    // Can go back to LOBBY from INTERMISSION
    await host.click('button[data-phase="LOBBY"]');
    await waitForBeamerScene(beamer, "sceneLobby");
  });

  test("host can reset game", async () => {
    const { host, beamer } = clients;

    await host.goto("/host.html");
    await beamer.goto("/beamer.html");

    await waitForConnection(host);

    // Navigate to Players panel and create some state
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "2");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");

    const tokensBefore = await getPlayerTokens(host);
    expect(tokensBefore).toHaveLength(2);

    // Navigate to Game Control panel for reset
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // Reset game (need to handle confirmation dialog)
    host.on("dialog", (dialog) => dialog.accept());
    await host.click('button:has-text("Spiel zurücksetzen")');

    // Wait for reset to complete
    await host.waitForTimeout(1000);

    // Verify reset - phase should be LOBBY
    await expect(host.locator("#overviewPhase")).toHaveText("LOBBY");

    // Beamer should show lobby
    await waitForBeamerScene(beamer, "sceneLobby");
  });

  test("panic mode blocks audience voting and shows overlay", async () => {
    const { host, beamer, players, audience } = clients;

    // ============================================
    // SETUP: Get to voting phase with submissions
    // ============================================
    console.log("Panic mode test: Setting up game...");

    // Navigate to pages
    await Promise.all([
      host.goto("/host.html"),
      beamer.goto("/beamer.html"),
      players[0].goto("/player.html"),
      audience[0].goto("/"),
    ]);

    await Promise.all([waitForConnection(host), waitForConnection(beamer)]);

    // Create player token
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");
    const tokens = await getPlayerTokens(host);

    // Player joins
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "PanicTester");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Audience joins
    await audience[0].click("#joinButton");
    await audience[0].waitForSelector("#waitingScreen.active");

    // Start round and add prompt
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");
    await host.click('button:has-text("Neue Runde starten")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Fragen")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Panic mode test question");
    await host.click('#prompts button:has-text("Frage hinzufügen")');
    await host.waitForTimeout(500);

    // Transition to WRITING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="PROMPT_SELECTION"]');
    await host.waitForTimeout(500);
    await host.click('button[data-phase="WRITING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Player submits answer
    await players[0].waitForSelector("#writingScreen.active", { timeout: 5000 });
    await players[0].fill("#answerInput", "Test answer for panic mode");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Mark submission as AI
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });
    const aiButton = host.locator('button:has-text("Als KI markieren")').first();
    if ((await aiButton.count()) > 0) {
      await aiButton.click();
      await host.waitForTimeout(500);
    }

    // Transition to REVEAL then VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await host.waitForTimeout(500);
    await host.click('button[data-phase="VOTING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("VOTING", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Audience sees voting screen normally
    // ============================================
    console.log("Panic mode test: Verifying normal voting screen...");

    await audience[0].waitForSelector("#votingScreen.active", { timeout: 5000 });
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Panic overlay should NOT be visible initially
    const overlayBefore = audience[0].locator("#panicModeOverlay");
    await expect(overlayBefore).toHaveCSS("display", "none");

    // ============================================
    // TEST: Host enables panic mode
    // ============================================
    console.log("Panic mode test: Enabling panic mode...");

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // Handle confirmation dialog for panic mode
    host.on("dialog", (dialog) => dialog.accept());

    // Click panic mode button
    await host.click("#panicModeBtn");
    await host.waitForTimeout(500);

    // Verify host shows panic mode active
    await expect(host.locator("#panicStatus")).toHaveText("AKTIV");

    // ============================================
    // TEST: Audience sees panic overlay
    // ============================================
    console.log("Panic mode test: Verifying panic overlay on audience...");

    // Panic overlay should now be visible
    const overlayAfter = audience[0].locator("#panicModeOverlay");
    await expect(overlayAfter).toBeVisible({ timeout: 5000 });

    // Verify overlay text
    await expect(
      audience[0].locator("#panicModeOverlay h3")
    ).toContainText("deaktiviert", { ignoreCase: true });

    // Vote button should be disabled
    const voteButton = audience[0].locator("#voteButton");
    await expect(voteButton).toBeDisabled();

    // ============================================
    // TEST: Host can disable panic mode
    // ============================================
    console.log("Panic mode test: Disabling panic mode...");

    await host.click("#panicModeBtn");
    await host.waitForTimeout(500);

    // Verify host shows panic mode inactive
    await expect(host.locator("#panicStatus")).toHaveText("Inaktiv");

    // Audience panic overlay should be hidden again
    await expect(overlayAfter).toHaveCSS("display", "none");

    console.log("Panic mode test completed successfully!");
  });
});
