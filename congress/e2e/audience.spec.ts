import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
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

  test("panic mode disconnects audience and blocks access", async () => {
    const { host, audience } = clients;

    // ============================================
    // SETUP: Host connects, audience joins
    // ============================================
    console.log("Panic mode test: Setting up game...");

    // Navigate to pages
    await host.goto("/host");
    await waitForConnection(host);

    // Audience joins during LOBBY phase
    await audience[0].goto("/");
    // Audience auto-creates/stores a voter token on load
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // Verify audience is connected (status dot should be green)
    await expect(audience[0].locator("#statusDot")).toHaveClass(/connected/, {
      timeout: 5000,
    });

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
    // TEST: Audience gets disconnected
    // ============================================
    console.log("Panic mode test: Verifying audience disconnected...");

    // Wait for disconnection (status dot should lose connected class)
    await expect(audience[0].locator("#statusDot")).not.toHaveClass(
      /connected/,
      { timeout: 5000 },
    );

    // ============================================
    // TEST: Audience HTTP access blocked
    // ============================================
    console.log("Panic mode test: Verifying HTTP access blocked...");

    // Try to reload the page - should get 403
    const response = await audience[0].goto("/");
    expect(response?.status()).toBe(403);

    // ============================================
    // TEST: Host disables panic mode
    // ============================================
    console.log("Panic mode test: Disabling panic mode...");

    await host.click("#panicModeBtn");
    await host.waitForTimeout(500);

    // Verify host shows panic mode inactive
    await expect(host.locator("#panicStatus")).toHaveText("Inaktiv");

    // ============================================
    // TEST: Audience can access again after panic mode disabled
    // ============================================
    console.log("Panic mode test: Verifying audience can access again...");

    // Reload should now work
    const responseAfter = await audience[0].goto("/");
    expect(responseAfter?.status()).toBe(200);

    // Audience should be able to join and see waiting screen
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // Verify reconnected
    await expect(audience[0].locator("#statusDot")).toHaveClass(/connected/, {
      timeout: 5000,
    });

    console.log("Panic mode test completed successfully!");
  });

  test("can only vote once per round (no change vote)", async () => {
    const { host, audience } = clients;

    // ============================================
    // SETUP: Full game flow to voting phase
    // ============================================
    console.log("Single vote test: Setting up game...");

    // Navigate to pages
    await host.goto("/host");
    await waitForConnection(host);
    await audience[0].goto("/");

    // Audience joins
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // Create player tokens
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");

    // Add and queue prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Test prompt for single vote test");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });
    await host.click("#startPromptSelectionBtn");

    // Wait for WRITING phase
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Add manual AI submission
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "AI answer for single vote test");
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

    // ============================================
    // TEST: Audience votes and sees confirmation
    // ============================================
    console.log("Single vote test: Audience voting...");

    await audience[0].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Verify warning message is shown
    await expect(audience[0].locator(".vote-warning")).toBeVisible();
    await expect(audience[0].locator(".vote-warning")).toHaveText(
      "Du kannst nur einmal abstimmen!",
    );

    // Cast vote
    const aiOptions = audience[0].locator("#aiAnswerOptions .answer-option");
    const funnyOptions = audience[0].locator(
      "#funnyAnswerOptions .answer-option",
    );
    await aiOptions.first().click();
    await funnyOptions.first().click();
    await audience[0].click("#voteButton");

    // ============================================
    // VERIFY: Confirmation screen without change vote button
    // ============================================
    console.log("Single vote test: Verifying confirmation screen...");

    await audience[0].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    // Verify "change vote" button does NOT exist
    const changeVoteButton = audience[0].locator(
      '#confirmedScreen button:has-text("Stimme ändern")',
    );
    await expect(changeVoteButton).not.toBeVisible();

    // Verify help text says to wait (not that vote can be changed)
    await expect(
      audience[0].locator('#confirmedScreen .help-text:has-text("Warte")'),
    ).toBeVisible();

    // Verify vote was recorded (check vote summary is shown)
    await expect(audience[0].locator("#summaryAiPick")).toBeVisible();
    await expect(audience[0].locator("#summaryFunnyPick")).toBeVisible();

    console.log("Single vote test completed successfully!");
  });
});
