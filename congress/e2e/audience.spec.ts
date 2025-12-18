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
});
