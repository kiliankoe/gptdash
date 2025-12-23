import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  waitForBeamerScene,
  resetGameState,
  createGameClients,
  closeContexts,
  getPlayerTokens,
} from "./test-utils";

/**
 * Beamer scene tests
 *
 * Tests beamer display for each game phase
 */
test.describe("Beamer", () => {
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

  test("displays correct scenes for each phase", async () => {
    const { host, beamer } = clients;

    await host.goto("/host");
    await beamer.goto("/beamer");

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

    // Add prompt to pool
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Scene test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");

    // Queue the prompt
    await host.locator("#hostPromptsList .queue-btn").first().click();

    // Wait for start button to become visible (triggered by server response)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Start prompt selection (auto-advances to WRITING with 1 prompt)
    await host.click("#startPromptSelectionBtn");
    await host.waitForTimeout(1000);

    // WRITING - should auto-advance since only 1 prompt
    await waitForBeamerScene(beamer, "sceneWriting");

    // Note: REVEAL, VOTING, and RESULTS require submissions/votes
    // Testing those phases in the full game flow test instead

    // Navigate to Game Control panel for phase transition buttons
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // INTERMISSION (from writing)
    await host.click('button[data-phase="INTERMISSION"]');
    await waitForBeamerScene(beamer, "sceneIntermission");

    // Can go back to LOBBY from INTERMISSION
    await host.click('button[data-phase="LOBBY"]');
    await waitForBeamerScene(beamer, "sceneLobby");
  });

  test("vote labels are hidden initially and can be revealed by host", async () => {
    test.setTimeout(60000);

    const { host, beamer, players } = clients;

    // Navigate to pages
    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer"),
      players[0].goto("/player"),
    ]);

    await Promise.all([waitForConnection(host), waitForConnection(beamer)]);

    // Create player and get token
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");
    const tokens = await getPlayerTokens(host);

    // Player joins with token
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active", { timeout: 5000 });
    await players[0].fill("#nameInput", "TestPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active", { timeout: 5000 });

    // Add and queue prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Vote label test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();

    // Start prompt selection (auto-advances to WRITING with 1 prompt)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });
    await host.click("#startPromptSelectionBtn");
    await waitForBeamerScene(beamer, "sceneWriting");

    // Player submits answer
    await players[0].waitForSelector("#answerInput", { timeout: 10000 });
    await players[0].fill("#answerInput", "Test answer for voting");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active", { timeout: 5000 });

    // Navigate to REVEAL and reveal answers
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await waitForBeamerScene(beamer, "sceneReveal");

    // Player should now see locked screen
    await players[0].waitForSelector("#lockedScreen.active", { timeout: 5000 });

    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // Transition to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("VOTING", {
      timeout: 5000,
    });
    await waitForBeamerScene(beamer, "sceneVoting");

    // Verify vote bars exist and labels are hidden
    const voteBars = beamer.locator(".vote-bar");
    await expect(voteBars.first()).toBeVisible();

    const voteBarLabels = beamer.locator(".vote-bar-label");
    await expect(voteBarLabels.first()).toHaveClass(/hidden/);

    // Host clicks reveal button
    await host.click('[data-action="reveal-vote-labels"]');

    // Verify labels are now visible (no hidden class)
    await expect(voteBarLabels.first()).not.toHaveClass(/hidden/);
  });
});
