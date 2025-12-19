import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  resetGameState,
  createGameClients,
  closeContexts,
  debugLog,
} from "./test-utils";

/**
 * XSS Vulnerability Tests
 *
 * Tests that user-submitted content containing XSS payloads
 * is properly escaped and does not execute on any page.
 */
test.describe("XSS Prevention", () => {
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

  test("audience prompt XSS payload does not execute on host", async () => {
    const { host, audience } = clients;

    // XSS payload that attempts to break out of title attribute
    // This is the exact payload reported by the user
    const XSS_PAYLOAD = '\'" onmouseover="alert(1)" <script>alert(2)</script>';

    // Track if any dialog (alert) appears - this would indicate XSS execution
    let dialogTriggered = false;
    host.on("dialog", async (dialog) => {
      debugLog(`XSS DETECTED! Dialog triggered: ${dialog.message()}`);
      dialogTriggered = true;
      await dialog.dismiss();
    });

    // ============================================
    // SETUP: Host connects
    // ============================================
    debugLog("XSS test: Setting up...");
    await host.goto("/host");
    await waitForConnection(host);

    // Audience joins
    await audience[0].goto("/");
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Audience submits malicious prompt
    // ============================================
    debugLog("XSS test: Submitting malicious prompt...");

    // Expand the prompt submission section (it's collapsed by default)
    await audience[0].click('[data-action="togglePromptSubmission"]');
    await audience[0].waitForSelector("#promptSubmissionContent:not(.collapsed)", {
      timeout: 2000,
    });

    // Find and fill the prompt input field
    await audience[0].fill("#promptInput", XSS_PAYLOAD);
    await audience[0].click("#submitPromptButton");

    // Wait for submission to process
    await audience[0].waitForTimeout(500);

    // ============================================
    // TEST: Host views the prompt
    // ============================================
    debugLog("XSS test: Host viewing prompts panel...");

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");

    // Wait for the prompt to appear in the audience prompts list
    await host.waitForSelector("#audiencePromptsList .prompt-row", {
      timeout: 5000,
    });

    // Hover over the prompt to trigger any onmouseover handlers
    await host.hover("#audiencePromptsList .prompt-row");
    await host.waitForTimeout(200);

    // Click on it to potentially trigger other handlers
    await host.click("#audiencePromptsList .prompt-row");
    await host.waitForTimeout(200);

    // ============================================
    // VERIFY: XSS did NOT execute
    // ============================================
    debugLog("XSS test: Verifying XSS did not execute...");

    expect(dialogTriggered).toBe(false);

    // Verify the payload appears as literal text (properly escaped)
    const promptText = await host
      .locator("#audiencePromptsList .prompt-text-preview")
      .first()
      .textContent();
    expect(promptText).toContain("onmouseover");

    debugLog("XSS test: Passed - no XSS execution detected");
  });
});
