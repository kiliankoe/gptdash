import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  waitForBeamerScene,
  getPlayerTokens,
  resetGameState,
  createGameClients,
  closeContexts,
} from "./test-utils";

/**
 * Host controls tests
 *
 * Tests host functionality: reset, state restoration
 */
test.describe("Host", () => {
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

  test("can reset game", async () => {
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

  test("state restoration after page reload", async () => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Create game state with prompts and submissions
    // ============================================
    console.log("Host state restoration test: Setting up game state...");

    await Promise.all([
      host.goto("/host.html"),
      players[0].goto("/player.html"),
    ]);

    await waitForConnection(host);

    // Create player tokens
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .player-status-card");

    const tokens = await getPlayerTokens(host);
    expect(tokens).toHaveLength(1);

    // Player joins and registers
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "StateTestPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Add multiple prompts to test list restoration
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");

    await host.fill("#promptText", "First test prompt for restoration");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForTimeout(300);

    await host.fill("#promptText", "Second test prompt for restoration");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForTimeout(300);

    await host.fill("#promptText", "Third test prompt for restoration");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForTimeout(300);

    // Verify prompts are listed before reload (pool prompts only, not queued)
    const promptsBeforeReload = host.locator("#promptsList .prompt-card");
    await expect(promptsBeforeReload).toHaveCount(3);

    // Queue the first prompt
    await host.click(
      '#promptsList .prompt-card button:has-text("+ Warteschlange")',
    );

    // Wait for start button to become visible (indicates queue operation completed)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Now we should have 2 prompts left in the pool (3 - 1 queued)
    await expect(host.locator("#promptsList .prompt-card")).toHaveCount(2);

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
    await players[0].fill("#answerInput", "Test answer for state restoration");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Verify submission appears in host view
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });

    const submissionsBeforeReload = host.locator(".submission-card");
    const submissionCountBefore = await submissionsBeforeReload.count();
    expect(submissionCountBefore).toBeGreaterThanOrEqual(1);

    // Verify player status shows submitted
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await expect(
      host.locator('.player-name:has-text("StateTestPlayer")'),
    ).toBeVisible();
    await expect(host.locator(".status-badge.submitted")).toHaveCount(1);

    // ============================================
    // TEST: Reload host page
    // ============================================
    console.log("Host state restoration test: Reloading host page...");

    await host.reload();
    await waitForConnection(host);

    // ============================================
    // VERIFY: Prompts list is restored after reload
    // ============================================
    console.log("Host state restoration test: Verifying prompts restored...");

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");

    // Should still have 2 prompts in the pool (the first one was selected and assigned to the round)
    const promptsAfterReload = host.locator("#promptsList .prompt-card");
    await expect(promptsAfterReload).toHaveCount(2, { timeout: 5000 });

    // Verify prompt content is preserved (Second and Third prompts remain in pool)
    await expect(
      host.locator('#promptsList .prompt-card:has-text("Second test prompt")'),
    ).toBeVisible();
    await expect(
      host.locator('#promptsList .prompt-card:has-text("Third test prompt")'),
    ).toBeVisible();

    // ============================================
    // VERIFY: Submissions list is restored after reload
    // ============================================
    console.log(
      "Host state restoration test: Verifying submissions restored...",
    );

    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");

    // Wait for submissions to load
    await host.waitForSelector(".submission-card", { timeout: 5000 });

    const submissionsAfterReload = host.locator(".submission-card");
    const submissionCountAfter = await submissionsAfterReload.count();
    expect(submissionCountAfter).toBe(submissionCountBefore);

    // Verify submission content is preserved
    await expect(
      host.locator(
        ".submission-card:has-text('Test answer for state restoration')",
      ),
    ).toBeVisible();

    // ============================================
    // VERIFY: Player status is restored after reload
    // ============================================
    console.log(
      "Host state restoration test: Verifying player status restored...",
    );

    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");

    // Player should still be shown with their name
    await expect(
      host.locator('.player-name:has-text("StateTestPlayer")'),
    ).toBeVisible({ timeout: 5000 });

    // Player should still show submitted status
    await expect(host.locator(".status-badge.submitted")).toHaveCount(1);

    // ============================================
    // VERIFY: Game phase is restored after reload
    // ============================================
    console.log(
      "Host state restoration test: Verifying game phase restored...",
    );

    // Phase should still be WRITING
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING");

    console.log("Host state restoration test completed successfully!");
  });
});
