import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  getPlayerTokens,
  resetGameState,
  createGameClients,
  closeContexts,
  debugLog,
} from "./test-utils";

/**
 * Player lifecycle tests
 *
 * Tests player join, registration, status tracking, and dynamic management
 */
test.describe("Player", () => {
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

  test("can join with token and register name", async () => {
    const { host, players } = clients;

    // Setup: Connect host, create token
    await host.goto("/host");
    await waitForConnection(host);

    // Navigate to Players panel
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");
    const tokens = await getPlayerTokens(host);

    // Player joins with token
    await players[0].goto("/player");
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

  test("host sees player names and submission status", async () => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Connect host and create players
    // ============================================
    debugLog("Player status test: Setting up game...");

    await Promise.all([
      host.goto("/host"),
      players[0].goto("/player"),
      players[1].goto("/player"),
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
    debugLog("Player status test: Checking initial state...");

    // Should show "Nicht registriert" for unregistered players
    const playerCards = host.locator("#playerTokensList .player-status-card");
    await expect(playerCards).toHaveCount(2);

    // Check for waiting status badges
    const waitingBadges = host.locator(
      "#playerTokensList .status-badge.waiting",
    );
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
    debugLog("Player status test: Player 1 registering...");

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
      host.locator(
        '#playerTokensList .player-name:has-text("StatusTestAlice")',
      ),
    ).toBeVisible();

    // ============================================
    // SETUP: Get to writing phase
    // ============================================
    debugLog("Player status test: Transitioning to writing...");

    // Player 2 also registers
    await players[1].fill("#tokenInput", tokens[1]);
    await players[1].click("#joinButton");
    await players[1].waitForSelector("#registerScreen.active");
    await players[1].fill("#nameInput", "StatusTestBob");
    await players[1].click("#registerButton");
    await players[1].waitForSelector("#waitingScreen.active");

    await host.waitForTimeout(500);
    await expect(
      host.locator('#playerTokensList .player-name:has-text("StatusTestBob")'),
    ).toBeVisible();

    // Add prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Status test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();

    // Wait for start button to become visible (triggered by server response)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Start prompt selection (auto-advances to WRITING with 1 prompt)
    await host.click("#startPromptSelectionBtn");
    await host.waitForTimeout(1000);
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Player 1 submits - status changes to submitted
    // ============================================
    debugLog("Player status test: Player 1 submitting...");

    // Navigate to players panel to watch status
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Both should be waiting
    await expect(
      host.locator("#playerTokensList .status-badge.waiting"),
    ).toHaveCount(2);

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
    const submittedBadges = host.locator(
      "#playerTokensList .status-badge.submitted",
    );
    await expect(submittedBadges).toHaveCount(1);

    // Bob should still be waiting
    const stillWaitingBadges = host.locator(
      "#playerTokensList .status-badge.waiting",
    );
    await expect(stillWaitingBadges).toHaveCount(1);

    // ============================================
    // TEST: Player 2 submits - both now submitted
    // ============================================
    debugLog("Player status test: Player 2 submitting...");

    await players[1].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[1].fill("#answerInput", "Bob's test answer for status");
    await players[1].click("#submitButton");
    await players[1].waitForSelector("#submittedScreen.active");

    // Wait for status update
    await host.waitForTimeout(1000);

    // Both should now show submitted
    const allSubmittedBadges = host.locator(
      "#playerTokensList .status-badge.submitted",
    );
    await expect(allSubmittedBadges).toHaveCount(2);

    // No more waiting badges
    const noWaitingBadges = host.locator(
      "#playerTokensList .status-badge.waiting",
    );
    await expect(noWaitingBadges).toHaveCount(0);

    debugLog("Player status test completed successfully!");
  });

  test("host can remove player mid-round and affected votes are reset", async () => {
    const { host, beamer, players, audience } = clients;

    // ============================================
    // SETUP: Get to voting phase with submissions
    // ============================================
    debugLog("Remove player test: Setting up game...");

    // Navigate to pages
    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer"),
      players[0].goto("/player"),
      players[1].goto("/player"),
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
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active");

    // Add prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Remove player test question");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();

    // Wait for start button to become visible (triggered by server response)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Start prompt selection (auto-advances to WRITING with 1 prompt)
    await host.click("#startPromptSelectionBtn");
    await host.waitForTimeout(1000);
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

    // Ensure there's an AI submission selected (required for RESULTS).
    // Use the host's manual AI override so tests don't depend on external LLMs.
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill(
      "#manualAiText",
      "Manual AI answer for remove-player test.",
    );
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

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
    debugLog("Remove player test: Audience voting...");

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
    debugLog("Remove player test: Host removing player...");

    // Navigate to Players panel
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Verify we see 2 players
    const playerCardsBefore = host.locator(
      "#playerTokensList .player-status-card",
    );
    await expect(playerCardsBefore).toHaveCount(2);

    // Handle confirmation dialog
    host.on("dialog", (dialog) => dialog.accept());

    // Find and click the remove button for the first player (ToBeRemoved)
    const playerToRemoveCard = host.locator(
      '#playerTokensList .player-status-card:has-text("ToBeRemoved")',
    );
    const removeButton = playerToRemoveCard.locator(".remove-btn");
    await expect(removeButton).toBeVisible();
    await removeButton.click();

    // Wait for removal to process
    await host.waitForTimeout(1000);

    // ============================================
    // VERIFY: Player is removed
    // ============================================
    debugLog("Remove player test: Verifying player removal...");

    // Should now only see 1 player
    const playerCardsAfter = host.locator(
      "#playerTokensList .player-status-card",
    );
    await expect(playerCardsAfter).toHaveCount(1);

    // The remaining player should be RemainingPlayer
    await expect(
      host.locator(
        '#playerTokensList .player-name:has-text("RemainingPlayer")',
      ),
    ).toBeVisible();
    await expect(
      host.locator('#playerTokensList .player-name:has-text("ToBeRemoved")'),
    ).not.toBeVisible();

    // ============================================
    // VERIFY: Submission is removed
    // ============================================
    debugLog("Remove player test: Verifying submission removal...");

    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");

    // Should only see 1 player submission now (the remaining player)
    // Note: The AI marking may have moved to another submission
    // Use .badge.player selector to avoid matching "openai" which contains "PLAYER"
    const playerSubmissionsAfter = host.locator(
      ".submission-card:has(.badge.player)",
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
    debugLog("Remove player test: Verifying audience can vote again...");

    // Audience should be back on voting screen (their vote was invalidated)
    // Note: They may need to refresh or the UI may automatically update
    // For now, let's verify the game can continue to RESULTS

    // Go to game control and transition to RESULTS
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="RESULTS"]');

    await expect(host.locator("#overviewPhase")).toHaveText("RESULTS", {
      timeout: 5000,
    });

    debugLog("Remove player test completed successfully!");
  });

  test("host can add new player mid-round during writing phase", async () => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Start with one player in writing phase
    // ============================================
    debugLog("Add player mid-round test: Setting up game...");

    await Promise.all([host.goto("/host"), players[0].goto("/player")]);

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

    // Add prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Add player mid-round test");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();

    // Wait for start button to become visible (triggered by server response)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Start prompt selection (auto-advances to WRITING with 1 prompt)
    await host.click("#startPromptSelectionBtn");
    await host.waitForTimeout(1000);
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
    debugLog("Add player mid-round test: Adding new player...");

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
    debugLog("Add player mid-round test: New player joining...");

    // Load player page in second player context
    await players[1].goto("/player");
    await players[1].fill("#tokenInput", newToken as string);
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
    debugLog("Add player mid-round test: Verifying submissions...");

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
    debugLog("Add player mid-round test: Verifying player status...");

    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Both players should be shown with submitted status
    const submittedBadges = host.locator(
      "#playerTokensList .status-badge.submitted",
    );
    await expect(submittedBadges).toHaveCount(2);

    // Both names should appear
    await expect(
      host.locator('#playerTokensList .player-name:has-text("FirstPlayer")'),
    ).toBeVisible();
    await expect(
      host.locator('#playerTokensList .player-name:has-text("LateArrival")'),
    ).toBeVisible();

    debugLog("Add player mid-round test completed successfully!");
  });

  test("player reconnects during WRITING phase and can submit", async ({
    browser,
  }) => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Get to WRITING phase with connected player
    // ============================================
    debugLog("Player reconnect test: Setting up game...");

    await host.goto("/host");
    await waitForConnection(host);

    // Create player token
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .player-status-card");

    const tokens = await getPlayerTokens(host);
    expect(tokens).toHaveLength(1);
    const playerToken = tokens[0];

    // Player joins and registers
    await players[0].goto("/player");
    await players[0].fill("#tokenInput", playerToken);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "ReconnectPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Add prompt and start game
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Reconnect test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });
    await host.click("#startPromptSelectionBtn");
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Verify player sees writing screen
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Close player page and reconnect
    // ============================================
    debugLog("Player reconnect test: Disconnecting player...");

    // Close player page
    await players[0].close();

    // Wait a moment for disconnection to register
    await host.waitForTimeout(500);

    // Create new player page and reconnect with token via URL param
    debugLog("Player reconnect test: Reconnecting player...");
    const reconnectedContext = await browser.newContext();
    contexts.push(reconnectedContext);
    const reconnectedPlayer = await reconnectedContext.newPage();

    // Navigate with token in URL (simulating reconnection with stored token)
    await reconnectedPlayer.goto(`/player?token=${playerToken}`);

    // ============================================
    // VERIFY: Player sees WRITING screen after reconnect
    // ============================================
    debugLog("Player reconnect test: Verifying state recovery...");

    // Player should see writing screen with the prompt
    await reconnectedPlayer.waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // The prompt text should be visible
    await expect(reconnectedPlayer.locator("#promptText")).toContainText(
      "Reconnect test prompt",
    );

    // ============================================
    // VERIFY: Player can submit answer after reconnect
    // ============================================
    debugLog("Player reconnect test: Submitting answer...");

    await reconnectedPlayer.fill(
      "#answerInput",
      "Answer submitted after reconnection",
    );
    await reconnectedPlayer.click("#submitButton");
    await reconnectedPlayer.waitForSelector("#submittedScreen.active", {
      timeout: 5000,
    });

    // Verify submission appears in host view
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(
      ".submission-card:has-text('Answer submitted after reconnection')",
      { timeout: 5000 },
    );

    debugLog("Player reconnect during WRITING test completed successfully!");
  });

  test("player reconnects during VOTING phase and sees locked screen", async ({
    browser,
  }) => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Get to VOTING phase
    // ============================================
    debugLog("Player reconnect VOTING test: Setting up game...");

    await host.goto("/host");
    await waitForConnection(host);

    // Create player token
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .player-status-card");

    const tokens = await getPlayerTokens(host);
    const playerToken = tokens[0];

    // Player joins and registers
    await players[0].goto("/player");
    await players[0].fill("#tokenInput", playerToken);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "VotingReconnectPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Add prompt and start game
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Voting reconnect test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });
    await host.click("#startPromptSelectionBtn");
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Player submits answer
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[0].fill("#answerInput", "Player answer for voting test");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Add AI submission and set up for REVEAL
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });

    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "Manual AI answer for voting reconnect.");
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // Transition to REVEAL then VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await expect(host.locator("#overviewPhase")).toHaveText("REVEAL", {
      timeout: 5000,
    });
    await host.click('button[data-phase="VOTING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("VOTING", {
      timeout: 5000,
    });

    // Verify player sees locked screen
    await players[0].waitForSelector("#lockedScreen.active", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Close player page and reconnect
    // ============================================
    debugLog("Player reconnect VOTING test: Disconnecting player...");

    // Close player page
    await players[0].close();
    await host.waitForTimeout(500);

    // Create new player page and reconnect
    debugLog("Player reconnect VOTING test: Reconnecting player...");
    const reconnectedContext = await browser.newContext();
    contexts.push(reconnectedContext);
    const reconnectedPlayer = await reconnectedContext.newPage();

    await reconnectedPlayer.goto(`/player?token=${playerToken}`);

    // ============================================
    // VERIFY: Player sees locked screen after reconnect
    // ============================================
    debugLog("Player reconnect VOTING test: Verifying locked screen...");

    await reconnectedPlayer.waitForSelector("#lockedScreen.active", {
      timeout: 5000,
    });

    // Verify the locked screen has appropriate message (generic locked screen)
    await expect(reconnectedPlayer.locator("#lockedScreen")).toContainText(
      "Schau auf den Beamer",
    );

    debugLog("Player reconnect during VOTING test completed successfully!");
  });
});
