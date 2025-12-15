import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  getPlayerTokens,
  resetGameState,
  createGameClients,
  closeContexts,
} from "./test-utils";

/**
 * Audience tests
 *
 * Tests audience join, voting, and panic mode functionality
 */
test.describe("Audience", () => {
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

  test("can join and see waiting screen", async () => {
    const { host, audience } = clients;

    // Setup minimal game - just start game
    await host.goto("/host");
    await waitForConnection(host);

    // Audience joins during LOBBY phase
    await audience[0].goto("/");
    // Audience auto-creates/stores a voter token on load and will typically
    // skip the welcome screen during LOBBY once the server sends state.
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }

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

  test("panic mode blocks voting and shows overlay", async () => {
    const { host, beamer, players, audience } = clients;

    // ============================================
    // SETUP: Get to voting phase with submissions
    // ============================================
    console.log("Panic mode test: Setting up game...");

    // Navigate to pages
    await Promise.all([
      host.goto("/host"),
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
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active");

    // Add prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Panic mode test question");
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
});
