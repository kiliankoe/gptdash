import { test, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  waitForBeamerScene,
  resetGameState,
  createGameClients,
  closeContexts,
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

    // Add prompt to pool
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Scene test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector(".prompt-card");

    // Queue the prompt
    await host.click('.prompt-card button:has-text("+ Warteschlange")');

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
});
