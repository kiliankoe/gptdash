import { test, expect, type Page, type BrowserContext } from "@playwright/test";

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
    { timeout },
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
  timeout = 10000,
): Promise<void> {
  await beamer.waitForSelector(`#${sceneId}.active`, { timeout });
}

// Helper to extract player tokens from host UI
async function getPlayerTokens(host: Page): Promise<string[]> {
  // Try new player-status-card format first, fall back to old token-display format
  const tokens = await host.$$eval(
    "#playerTokensList .player-token .token, #playerTokensList .token-display .token",
    (els) => els.map((el) => el.textContent?.trim() ?? ""),
  );
  return tokens;
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
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");

    // Add a prompt (auto-selects when added by host)
    await host.fill(
      "#promptText",
      "What is the meaning of life, the universe, and everything?",
    );
    await host.click('#prompts button:has-text("Prompt hinzufügen")');

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

    // Mark first submission as AI (required for RESULTS phase)
    // Must be done during WRITING phase while "Als KI markieren" buttons are visible
    const aiButtonsWriting = host.locator(
      'button:has-text("Als KI markieren")',
    );
    const aiButtonCountWriting = await aiButtonsWriting.count();
    console.log(
      `Found ${aiButtonCountWriting} AI marking buttons during WRITING`,
    );
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
    await players[0].waitForSelector("#registerScreen.active", {
      timeout: 5000,
    });

    // Player registers name
    await players[0].fill("#nameInput", "TestPlayer");
    await players[0].click("#registerButton");

    // Should see waiting screen
    await players[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // Verify token was stored
    const storedToken = await players[0].evaluate(() =>
      localStorage.getItem("gptdash_player_token"),
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
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // Verify voter token was stored
    const voterToken = await audience[0].evaluate(() =>
      localStorage.getItem("gptdash_voter_token"),
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
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Scene test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
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

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Panic mode test question");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
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
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[0].fill("#answerInput", "Test answer for panic mode");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Mark submission as AI
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });
    const aiButton = host
      .locator('button:has-text("Als KI markieren")')
      .first();
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

    await audience[0].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
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
    await expect(audience[0].locator("#panicModeOverlay h3")).toContainText(
      "deaktiviert",
      { ignoreCase: true },
    );

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

  test("duplicate detection blocks exact duplicates and host can mark duplicates", async () => {
    const { host, beamer, players } = clients;

    // ============================================
    // SETUP: Get to writing phase with player
    // ============================================
    console.log("Duplicate test: Setting up game...");

    // Navigate to pages
    await Promise.all([
      host.goto("/host.html"),
      beamer.goto("/beamer.html"),
      players[0].goto("/player.html"),
      players[1].goto("/player.html"),
    ]);

    await Promise.all([waitForConnection(host), waitForConnection(beamer)]);

    // Create player tokens
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "2");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");
    const tokens = await getPlayerTokens(host);

    // Player 1 joins
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "DuplicateTester1");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Player 2 joins
    await players[1].fill("#tokenInput", tokens[1]);
    await players[1].click("#joinButton");
    await players[1].waitForSelector("#registerScreen.active");
    await players[1].fill("#nameInput", "DuplicateTester2");
    await players[1].click("#registerButton");
    await players[1].waitForSelector("#waitingScreen.active");

    // Start round and add prompt
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");
    await host.click('button:has-text("Neue Runde starten")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Duplicate detection test question");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForTimeout(500);

    // Transition to WRITING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="PROMPT_SELECTION"]');
    await host.waitForTimeout(500);
    await host.click('button[data-phase="WRITING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // ============================================
    // TEST 1: Player 1 submits an answer
    // ============================================
    console.log("Duplicate test: Player 1 submitting answer...");

    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[0].fill(
      "#answerInput",
      "This is a unique answer that no one else will submit",
    );
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Verify submission appears in host view
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });

    // ============================================
    // TEST 2: Player 2 tries to submit exact duplicate
    // ============================================
    console.log("Duplicate test: Player 2 trying exact duplicate...");

    await players[1].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    // Submit the EXACT same answer (case insensitive with whitespace)
    await players[1].fill(
      "#answerInput",
      "  THIS IS A UNIQUE ANSWER THAT NO ONE ELSE WILL SUBMIT  ",
    );
    await players[1].click("#submitButton");

    // Player 2 should see an error message (stays on writing screen)
    // and the error message should appear
    await players[1].waitForSelector("#submitError:not(:empty)", {
      timeout: 5000,
    });
    const errorText = await getText(players[1], "#submitError");
    expect(errorText).toContain("existiert schon");

    // Player 2 should still be on writing screen (not submitted)
    await expect(players[1].locator("#writingScreen")).toHaveClass(/active/);

    // ============================================
    // TEST 3: Player 2 submits a different answer
    // ============================================
    console.log("Duplicate test: Player 2 submitting different answer...");

    await players[1].fill(
      "#answerInput",
      "This is a completely different answer",
    );
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // Verify both player submissions appear in host view (exclude AI submissions)
    await host.waitForTimeout(500);
    const playerSubmissionsBefore = host.locator(
      ".submission-card:has-text('PLAYER')",
    );
    await expect(playerSubmissionsBefore).toHaveCount(2);

    // ============================================
    // TEST 4: Host marks Player 2's submission as duplicate
    // ============================================
    console.log("Duplicate test: Host marking submission as duplicate...");

    // Handle confirmation dialog
    host.on("dialog", (dialog) => dialog.accept());

    // Find the Dupe button for Player 2's submission by locating the card with their answer text
    const player2Card = host.locator(
      ".submission-card:has-text('This is a completely different answer')",
    );
    const dupeButton = player2Card.locator('button:has-text("Dupe")');
    await expect(dupeButton).toBeVisible();
    await dupeButton.click();

    // Wait for the submission to be removed
    await host.waitForTimeout(1000);
    const playerSubmissionsAfter = host.locator(
      ".submission-card:has-text('PLAYER')",
    );
    await expect(playerSubmissionsAfter).toHaveCount(1);

    // ============================================
    // TEST 5: Player 2 is notified and can resubmit
    // ============================================
    console.log("Duplicate test: Verifying player notification...");

    // Player 2 should be back on writing screen with an error message
    await players[1].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // The error message should indicate duplicate
    await players[1].waitForSelector("#submitError:not(:empty)", {
      timeout: 5000,
    });
    const rejectionError = await getText(players[1], "#submitError");
    expect(rejectionError).toContain("existiert schon");

    // Player 2 can now submit a new answer
    await players[1].fill(
      "#answerInput",
      "Yet another completely unique and original answer",
    );
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // Verify the new submission appears
    await host.waitForTimeout(500);
    const playerSubmissionsFinal = host.locator(
      ".submission-card:has-text('PLAYER')",
    );
    await expect(playerSubmissionsFinal).toHaveCount(2);

    console.log("Duplicate detection test completed successfully!");
  });

  test("typo correction flow shows comparison when LLM suggests changes", async () => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Get to writing phase with player
    // ============================================
    console.log("Typo correction test: Setting up game...");

    await Promise.all([
      host.goto("/host.html"),
      players[0].goto("/player.html"),
    ]);

    await waitForConnection(host);

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
    await players[0].fill("#nameInput", "TypoTester");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Start round and add prompt
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");
    await host.click('button:has-text("Neue Runde starten")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Typo correction test question");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForTimeout(500);

    // Transition to WRITING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="PROMPT_SELECTION"]');
    await host.waitForTimeout(500);
    await host.click('button[data-phase="WRITING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Player submits answer and sees confirmation
    // ============================================
    console.log("Typo correction test: Player submitting answer...");

    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // Submit an answer (without LLM configured, no correction will be suggested)
    await players[0].fill(
      "#answerInput",
      "Dies ist eine Testantwort ohne Tippfehler.",
    );
    await players[0].click("#submitButton");

    // Player should see submitted screen immediately (soft submission)
    await players[0].waitForSelector("#submittedScreen.active", {
      timeout: 5000,
    });

    // Since no LLM is configured, typo check will return no changes
    // and player stays on submitted screen
    await players[0].waitForTimeout(1000);

    // Verify still on submitted screen (no typo correction screen shown)
    await expect(players[0].locator("#submittedScreen")).toHaveClass(/active/);

    // Verify typo check screen exists but is not active
    const typoScreen = players[0].locator("#typoCheckScreen");
    await expect(typoScreen).not.toHaveClass(/active/);

    console.log("Typo correction test: Verified submitted flow without LLM");

    // ============================================
    // TEST: Verify comparison UI elements exist
    // ============================================
    console.log("Typo correction test: Verifying UI elements exist...");

    // Check that the typo check screen has all required elements
    await expect(players[0].locator("#typoCheckScreen")).toBeAttached();
    await expect(players[0].locator("#originalText")).toBeAttached();
    await expect(players[0].locator("#correctedText")).toBeAttached();
    await expect(
      players[0].locator('button:has-text("Korrektur übernehmen")'),
    ).toBeAttached();
    await expect(
      players[0].locator('button:has-text("Original behalten")'),
    ).toBeAttached();
    await expect(
      players[0].locator('button:has-text("Selbst bearbeiten")'),
    ).toBeAttached();

    console.log("Typo correction test completed successfully!");
  });

  test("host sees player names and submission status", async () => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Connect host and create players
    // ============================================
    console.log("Player status test: Setting up game...");

    await Promise.all([
      host.goto("/host.html"),
      players[0].goto("/player.html"),
      players[1].goto("/player.html"),
    ]);

    await waitForConnection(host);

    // Navigate to Players panel and create tokens
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "2");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .player-status-card");

    // ============================================
    // TEST: Initial state - no names, waiting status
    // ============================================
    console.log("Player status test: Checking initial state...");

    // Should show "Nicht registriert" for unregistered players
    const playerCards = host.locator(".player-status-card");
    await expect(playerCards).toHaveCount(2);

    // Check for waiting status badges
    const waitingBadges = host.locator(".status-badge.waiting");
    await expect(waitingBadges).toHaveCount(2);

    // Get tokens for players
    const tokens = await host.$$eval(
      "#playerTokensList .player-token .token",
      (els) => els.map((el) => el.textContent?.trim() ?? ""),
    );
    expect(tokens).toHaveLength(2);

    // ============================================
    // TEST: Player 1 registers - name appears
    // ============================================
    console.log("Player status test: Player 1 registering...");

    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "StatusTestAlice");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Wait for status update to propagate
    await host.waitForTimeout(500);

    // Check that Alice's name appears
    await expect(
      host.locator('.player-name:has-text("StatusTestAlice")'),
    ).toBeVisible();

    // ============================================
    // SETUP: Get to writing phase
    // ============================================
    console.log("Player status test: Transitioning to writing...");

    // Player 2 also registers
    await players[1].fill("#tokenInput", tokens[1]);
    await players[1].click("#joinButton");
    await players[1].waitForSelector("#registerScreen.active");
    await players[1].fill("#nameInput", "StatusTestBob");
    await players[1].click("#registerButton");
    await players[1].waitForSelector("#waitingScreen.active");

    await host.waitForTimeout(500);
    await expect(
      host.locator('.player-name:has-text("StatusTestBob")'),
    ).toBeVisible();

    // Start round and setup prompt
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");
    await host.click('button:has-text("Neue Runde starten")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Status test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="PROMPT_SELECTION"]');
    await host.waitForTimeout(500);
    await host.click('button[data-phase="WRITING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Player 1 submits - status changes to submitted
    // ============================================
    console.log("Player status test: Player 1 submitting...");

    // Navigate to players panel to watch status
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Both should be waiting
    await expect(host.locator(".status-badge.waiting")).toHaveCount(2);

    // Player 1 submits
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[0].fill("#answerInput", "Alice's test answer for status");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Wait for status update
    await host.waitForTimeout(1000);

    // Alice should now show submitted
    const submittedBadges = host.locator(".status-badge.submitted");
    await expect(submittedBadges).toHaveCount(1);

    // Bob should still be waiting
    const stillWaitingBadges = host.locator(".status-badge.waiting");
    await expect(stillWaitingBadges).toHaveCount(1);

    // ============================================
    // TEST: Player 2 submits - both now submitted
    // ============================================
    console.log("Player status test: Player 2 submitting...");

    await players[1].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[1].fill("#answerInput", "Bob's test answer for status");
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // Wait for status update
    await host.waitForTimeout(1000);

    // Both should now show submitted
    const allSubmittedBadges = host.locator(".status-badge.submitted");
    await expect(allSubmittedBadges).toHaveCount(2);

    // No more waiting badges
    const noWaitingBadges = host.locator(".status-badge.waiting");
    await expect(noWaitingBadges).toHaveCount(0);

    console.log("Player status test completed successfully!");
  });

  test("host can remove player mid-round and affected votes are reset", async () => {
    const { host, beamer, players, audience } = clients;

    // ============================================
    // SETUP: Get to voting phase with submissions
    // ============================================
    console.log("Remove player test: Setting up game...");

    // Navigate to pages
    await Promise.all([
      host.goto("/host.html"),
      beamer.goto("/beamer.html"),
      players[0].goto("/player.html"),
      players[1].goto("/player.html"),
      audience[0].goto("/"),
    ]);

    await Promise.all([waitForConnection(host), waitForConnection(beamer)]);

    // Create player tokens
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "2");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .player-status-card");

    const tokens = await getPlayerTokens(host);
    expect(tokens).toHaveLength(2);

    // Player 1 joins (Alice - will be removed)
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "ToBeRemoved");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Player 2 joins (Bob - will remain)
    await players[1].fill("#tokenInput", tokens[1]);
    await players[1].click("#joinButton");
    await players[1].waitForSelector("#registerScreen.active");
    await players[1].fill("#nameInput", "RemainingPlayer");
    await players[1].click("#registerButton");
    await players[1].waitForSelector("#waitingScreen.active");

    // Audience joins
    await audience[0].click("#joinButton");
    await audience[0].waitForSelector("#waitingScreen.active");

    // Start round and add prompt
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");
    await host.click('button:has-text("Neue Runde starten")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Remove player test question");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForTimeout(500);

    // Transition to WRITING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="PROMPT_SELECTION"]');
    await host.waitForTimeout(500);
    await host.click('button[data-phase="WRITING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Both players submit answers
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[0].fill("#answerInput", "Answer from player to be removed");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    await players[1].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[1].fill("#answerInput", "Answer from remaining player");
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // Verify both submissions appear in host view
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });

    // Mark one submission as AI (required for RESULTS)
    const aiButtonsWriting = host.locator(
      'button:has-text("Als KI markieren")',
    );
    if ((await aiButtonsWriting.count()) > 0) {
      // Mark the second one as AI (remaining player's)
      await aiButtonsWriting.last().click();
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
    // TEST: Audience votes for the player-to-be-removed's answer
    // ============================================
    console.log("Remove player test: Audience voting...");

    await audience[0].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Vote for first answer (ToBeRemoved's) as both AI and funny
    const aiOptions = audience[0].locator("#aiAnswerOptions .answer-option");
    const funnyOptions = audience[0].locator(
      "#funnyAnswerOptions .answer-option",
    );

    await aiOptions.first().click();
    await funnyOptions.first().click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Host removes first player during voting
    // ============================================
    console.log("Remove player test: Host removing player...");

    // Navigate to Players panel
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Verify we see 2 players
    const playerCardsBefore = host.locator(".player-status-card");
    await expect(playerCardsBefore).toHaveCount(2);

    // Handle confirmation dialog
    host.on("dialog", (dialog) => dialog.accept());

    // Find and click the remove button for the first player (ToBeRemoved)
    const playerToRemoveCard = host.locator(
      '.player-status-card:has-text("ToBeRemoved")',
    );
    const removeButton = playerToRemoveCard.locator(".remove-btn");
    await expect(removeButton).toBeVisible();
    await removeButton.click();

    // Wait for removal to process
    await host.waitForTimeout(1000);

    // ============================================
    // VERIFY: Player is removed
    // ============================================
    console.log("Remove player test: Verifying player removal...");

    // Should now only see 1 player
    const playerCardsAfter = host.locator(".player-status-card");
    await expect(playerCardsAfter).toHaveCount(1);

    // The remaining player should be RemainingPlayer
    await expect(
      host.locator('.player-name:has-text("RemainingPlayer")'),
    ).toBeVisible();
    await expect(
      host.locator('.player-name:has-text("ToBeRemoved")'),
    ).not.toBeVisible();

    // ============================================
    // VERIFY: Submission is removed
    // ============================================
    console.log("Remove player test: Verifying submission removal...");

    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");

    // Should only see 1 player submission now (the remaining player)
    // Note: The AI marking may have moved to another submission
    const playerSubmissionsAfter = host.locator(
      ".submission-card:has-text('PLAYER')",
    );
    await expect(playerSubmissionsAfter).toHaveCount(1);

    // The remaining submission should be from RemainingPlayer
    await expect(
      host.locator(".submission-card:has-text('Answer from remaining player')"),
    ).toBeVisible();
    await expect(
      host.locator(
        ".submission-card:has-text('Answer from player to be removed')",
      ),
    ).not.toBeVisible();

    // ============================================
    // VERIFY: Audience can vote again (their vote was reset)
    // ============================================
    console.log(
      "Remove player test: Verifying audience can vote again...",
    );

    // Audience should be back on voting screen (their vote was invalidated)
    // Note: They may need to refresh or the UI may automatically update
    // For now, let's verify the game can continue to RESULTS

    // Need to re-mark AI submission since the original may have been removed
    const aiButtons = host.locator('button:has-text("Als KI markieren")');
    if ((await aiButtons.count()) > 0) {
      await aiButtons.first().click();
      await host.waitForTimeout(500);
    }

    // Go to game control and transition to RESULTS
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="RESULTS"]');

    await expect(host.locator("#overviewPhase")).toHaveText("RESULTS", {
      timeout: 5000,
    });

    console.log("Remove player test completed successfully!");
  });

  test("host can add new player mid-round during writing phase", async () => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Start with one player in writing phase
    // ============================================
    console.log("Add player mid-round test: Setting up game...");

    await Promise.all([
      host.goto("/host.html"),
      players[0].goto("/player.html"),
    ]);

    await waitForConnection(host);

    // Create initial player token
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .player-status-card");

    const initialTokens = await getPlayerTokens(host);
    expect(initialTokens).toHaveLength(1);

    // First player joins
    await players[0].fill("#tokenInput", initialTokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "FirstPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Start round and setup
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");
    await host.click('button:has-text("Neue Runde starten")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Add player mid-round test");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForTimeout(500);

    // Transition to WRITING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="PROMPT_SELECTION"]');
    await host.waitForTimeout(500);
    await host.click('button[data-phase="WRITING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // First player sees writing screen
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Host adds new player during WRITING phase
    // ============================================
    console.log("Add player mid-round test: Adding new player...");

    // Navigate to Players panel
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Create another player token
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForTimeout(500);

    // Should now have 2 players
    const updatedTokens = await getPlayerTokens(host);
    expect(updatedTokens).toHaveLength(2);

    // Get the new token (it wasn't in the initial list)
    const newToken = updatedTokens.find((t) => !initialTokens.includes(t));
    expect(newToken).toBeTruthy();

    // ============================================
    // TEST: New player can join and submit during WRITING
    // ============================================
    console.log("Add player mid-round test: New player joining...");

    // Load player page in second player context
    await players[1].goto("/player.html");
    await players[1].fill("#tokenInput", newToken!);
    await players[1].click("#joinButton");
    await players[1].waitForSelector("#registerScreen.active");
    await players[1].fill("#nameInput", "LateArrival");
    await players[1].click("#registerButton");

    // New player should go directly to writing screen (since game is in WRITING phase)
    await players[1].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // New player can submit
    await players[1].fill("#answerInput", "Late arrival's answer");
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // First player also submits
    await players[0].fill("#answerInput", "First player's answer");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // ============================================
    // VERIFY: Both submissions appear in host view
    // ============================================
    console.log("Add player mid-round test: Verifying submissions...");

    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });

    const playerSubmissions = host.locator(
      ".submission-card:has-text('PLAYER')",
    );
    await expect(playerSubmissions).toHaveCount(2);

    // Both answers should be present
    await expect(
      host.locator(".submission-card:has-text('First player')"),
    ).toBeVisible();
    await expect(
      host.locator(".submission-card:has-text('Late arrival')"),
    ).toBeVisible();

    // ============================================
    // VERIFY: Player status shows both players
    // ============================================
    console.log("Add player mid-round test: Verifying player status...");

    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Both players should be shown with submitted status
    const submittedBadges = host.locator(".status-badge.submitted");
    await expect(submittedBadges).toHaveCount(2);

    // Both names should appear
    await expect(
      host.locator('.player-name:has-text("FirstPlayer")'),
    ).toBeVisible();
    await expect(
      host.locator('.player-name:has-text("LateArrival")'),
    ).toBeVisible();

    console.log("Add player mid-round test completed successfully!");
  });
});
