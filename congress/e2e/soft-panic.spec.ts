import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  resetGameState,
  createGameClients,
  closeContexts,
  debugLog,
  setupGameToWriting,
  getPlayerTokens,
} from "./test-utils";

/**
 * Soft Panic Mode tests
 *
 * Tests the soft panic mode which blocks prompt submissions
 * while allowing normal voting to continue.
 */
test.describe("Soft Panic Mode", () => {
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

  test("soft panic blocks prompt submission", async () => {
    const { host, audience } = clients;

    // ============================================
    // SETUP: Host connects, audience joins
    // ============================================
    debugLog("Soft panic test: Setting up game...");

    await host.goto("/host");
    await waitForConnection(host);

    await audience[0].goto("/");
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // Verify audience is connected
    await expect(audience[0].locator("#statusDot")).toHaveClass(/connected/, {
      timeout: 5000,
    });

    // ============================================
    // TEST: Audience can submit prompt before soft panic
    // ============================================
    debugLog("Soft panic test: Verifying prompt submission works initially...");

    // Expand prompt submission section
    await audience[0].click('[data-action="togglePromptSubmission"]');
    await audience[0].waitForSelector(
      "#promptSubmissionContent:not(.collapsed)",
      { timeout: 2000 },
    );

    // Verify input is enabled
    await expect(audience[0].locator("#promptInput")).toBeEnabled();
    await expect(audience[0].locator("#submitPromptButton")).toBeEnabled();

    // Disabled notice should be hidden
    await expect(
      audience[0].locator("#promptDisabledNotice"),
    ).not.toBeVisible();

    // ============================================
    // TEST: Host enables soft panic mode
    // ============================================
    debugLog("Soft panic test: Enabling soft panic mode...");

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // Handle confirmation dialog
    host.once("dialog", (dialog) => dialog.accept());

    // Click soft panic mode button
    await host.click("#softPanicModeBtn");
    await host.waitForTimeout(500);

    // Verify host shows soft panic mode active
    await expect(host.locator("#softPanicStatus")).toHaveText("AKTIV");

    // ============================================
    // TEST: Audience prompt submission is disabled
    // ============================================
    debugLog("Soft panic test: Verifying prompt submission is disabled...");

    // Wait for the UI update on audience side
    await audience[0].waitForTimeout(500);

    // Verify input is disabled
    await expect(audience[0].locator("#promptInput")).toBeDisabled();
    await expect(audience[0].locator("#submitPromptButton")).toBeDisabled();

    // Disabled notice should be visible
    await expect(audience[0].locator("#promptDisabledNotice")).toBeVisible();

    // ============================================
    // TEST: Host disables soft panic mode
    // ============================================
    debugLog("Soft panic test: Disabling soft panic mode...");

    host.once("dialog", (dialog) => dialog.accept());
    await host.click("#softPanicModeBtn");
    await host.waitForTimeout(500);

    // Verify host shows soft panic mode inactive
    await expect(host.locator("#softPanicStatus")).toHaveText("Inaktiv");

    // ============================================
    // TEST: Audience prompt submission is re-enabled
    // ============================================
    debugLog("Soft panic test: Verifying prompt submission is re-enabled...");

    await audience[0].waitForTimeout(500);

    // Verify input is enabled again
    await expect(audience[0].locator("#promptInput")).toBeEnabled();
    await expect(audience[0].locator("#submitPromptButton")).toBeEnabled();

    // Disabled notice should be hidden
    await expect(
      audience[0].locator("#promptDisabledNotice"),
    ).not.toBeVisible();

    debugLog("Soft panic test completed successfully!");
  });

  test("soft panic allows regular voting during VOTING phase", async ({}, testInfo) => {
    testInfo.setTimeout(90000);
    const { host, audience, players } = clients;

    // ============================================
    // SETUP: Full game flow to VOTING phase using helper
    // ============================================
    debugLog("Soft panic voting test: Setting up game...");

    await host.goto("/host");
    await waitForConnection(host);

    // Use the helper to set up game to WRITING phase
    await setupGameToWriting(host, players, ["Alice", "Bob"]);

    // Audience joins
    await audience[0].goto("/");
    await waitForConnection(audience[0]);
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    // Wait for WRITING phase
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 10000,
    });

    // Players submit answers
    for (let i = 0; i < 2; i++) {
      await players[i].waitForSelector("#writingScreen.active", {
        timeout: 10000,
      });
      await players[i].fill("#answerInput", `Answer from Player${i + 1}`);
      await players[i].click("#submitButton");
      await players[i].waitForSelector("#submittedScreen.active");
    }

    // Add manual AI answer (LLM is disabled in e2e tests)
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "AI answer for soft panic test");
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
    // TEST: Enable soft panic mode during VOTING
    // ============================================
    debugLog("Soft panic voting test: Enabling soft panic mode...");

    host.once("dialog", (dialog) => dialog.accept());
    await host.click("#softPanicModeBtn");
    await host.waitForTimeout(500);
    await expect(host.locator("#softPanicStatus")).toHaveText("AKTIV");

    // ============================================
    // TEST: Audience can still vote
    // ============================================
    debugLog("Soft panic voting test: Verifying audience can vote...");

    // Audience should see voting screen with answer options
    await audience[0].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Select first answer as AI pick
    await audience[0]
      .locator("#aiAnswerOptions .answer-option")
      .first()
      .click();

    // Select second answer as funny pick
    await audience[0]
      .locator("#funnyAnswerOptions .answer-option")
      .nth(1)
      .click();

    // Submit vote
    await audience[0].click("#voteButton");

    // Should see confirmed screen
    await audience[0].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    debugLog("Soft panic voting test completed successfully!");
  });

  test("soft panic resets with game reset", async () => {
    const { host } = clients;

    // ============================================
    // SETUP: Host connects
    // ============================================
    debugLog("Soft panic reset test: Setting up...");

    await host.goto("/host");
    await waitForConnection(host);

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // ============================================
    // TEST: Enable soft panic mode
    // ============================================
    debugLog("Soft panic reset test: Enabling soft panic mode...");

    host.once("dialog", (dialog) => dialog.accept());

    await host.click("#softPanicModeBtn");
    await host.waitForTimeout(500);
    await expect(host.locator("#softPanicStatus")).toHaveText("AKTIV");

    // ============================================
    // TEST: Reset game
    // ============================================
    debugLog("Soft panic reset test: Resetting game...");

    // Set up dialog handler for reset confirmation
    host.once("dialog", (dialog) => dialog.accept());

    await host.click('button:has-text("Spiel zurücksetzen")');
    await host.waitForTimeout(1000);

    // ============================================
    // TEST: Soft panic should be disabled
    // ============================================
    debugLog("Soft panic reset test: Verifying soft panic is disabled...");

    await expect(host.locator("#softPanicStatus")).toHaveText("Inaktiv");

    debugLog("Soft panic reset test completed successfully!");
  });

  test("soft panic mode is independent from regular panic mode", async () => {
    const { host, audience } = clients;

    // ============================================
    // SETUP
    // ============================================
    debugLog("Independence test: Setting up...");

    await host.goto("/host");
    await waitForConnection(host);

    await audience[0].goto("/");
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.waitForSelector("#game.active");

    // ============================================
    // TEST: Enable soft panic only
    // ============================================
    debugLog("Independence test: Enabling soft panic only...");

    host.once("dialog", (dialog) => dialog.accept());
    await host.click("#softPanicModeBtn");
    await host.waitForTimeout(500);

    await expect(host.locator("#softPanicStatus")).toHaveText("AKTIV");
    await expect(host.locator("#panicStatus")).toHaveText("Inaktiv");

    // Audience should still be connected (soft panic doesn't disconnect)
    await expect(audience[0].locator("#statusDot")).toHaveClass(/connected/, {
      timeout: 5000,
    });

    // ============================================
    // TEST: Enable regular panic (both active)
    // ============================================
    debugLog("Independence test: Enabling regular panic too...");

    host.once("dialog", (dialog) => dialog.accept());
    await host.click("#panicModeBtn");
    await host.waitForTimeout(500);

    await expect(host.locator("#softPanicStatus")).toHaveText("AKTIV");
    await expect(host.locator("#panicStatus")).toHaveText("AKTIV");

    // Regular panic disconnects audience
    await expect(audience[0].locator("#statusDot")).not.toHaveClass(
      /connected/,
      { timeout: 5000 },
    );

    // ============================================
    // TEST: Disable regular panic (soft panic still active)
    // ============================================
    debugLog("Independence test: Disabling regular panic...");

    host.once("dialog", (dialog) => dialog.accept());
    await host.click("#panicModeBtn");
    await host.waitForTimeout(500);

    await expect(host.locator("#softPanicStatus")).toHaveText("AKTIV");
    await expect(host.locator("#panicStatus")).toHaveText("Inaktiv");

    debugLog("Independence test completed successfully!");
  });
});
