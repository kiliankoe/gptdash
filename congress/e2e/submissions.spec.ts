import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  getText,
  getPlayerTokens,
  resetGameState,
  createGameClients,
  closeContexts,
  debugLog,
} from "./test-utils";

/**
 * Submission tests
 *
 * Tests submission handling, duplicate detection, and typo correction
 */
test.describe("Submissions", () => {
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

  test("duplicate detection blocks exact duplicates and host can mark duplicates", async () => {
    const { host, beamer, players } = clients;

    // ============================================
    // SETUP: Get to writing phase with player
    // ============================================
    debugLog("Duplicate test: Setting up game...");

    // Navigate to pages
    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer"),
      players[0].goto("/player"),
      players[1].goto("/player"),
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

    // Add prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Duplicate detection test question");
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
    // TEST 1: Player 1 submits an answer
    // ============================================
    debugLog("Duplicate test: Player 1 submitting answer...");

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
    debugLog("Duplicate test: Player 2 trying exact duplicate...");

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
    debugLog("Duplicate test: Player 2 submitting different answer...");

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
    debugLog("Duplicate test: Host marking submission as duplicate...");

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
    debugLog("Duplicate test: Verifying player notification...");

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

    debugLog("Duplicate detection test completed successfully!");
  });

  test("typo correction flow shows comparison when LLM suggests changes", async () => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Get to writing phase with player
    // ============================================
    debugLog("Typo correction test: Setting up game...");

    await Promise.all([host.goto("/host"), players[0].goto("/player")]);

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

    // Add prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Typo correction test question");
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
    // TEST: Player submits answer and sees confirmation
    // ============================================
    debugLog("Typo correction test: Player submitting answer...");

    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // Submit an answer
    await players[0].fill(
      "#answerInput",
      "Dies ist eine Testantwort ohne Tippfehler.",
    );
    await players[0].click("#submitButton");

    // Player should see submitted screen immediately (soft submission)
    await players[0].waitForSelector("#submittedScreen.active", {
      timeout: 5000,
    });

    // Wait for typo check to complete (runs in background)
    // If LLM is configured and suggests changes, player moves to typoCheckScreen
    // If no LLM or no changes suggested, player stays on submittedScreen
    await players[0].waitForTimeout(2000);

    // Check which screen is active - both are valid outcomes
    const isOnSubmittedScreen = await players[0]
      .locator("#submittedScreen.active")
      .isVisible();
    const isOnTypoCheckScreen = await players[0]
      .locator("#typoCheckScreen.active")
      .isVisible();

    // One of them must be active
    expect(isOnSubmittedScreen || isOnTypoCheckScreen).toBe(true);

    debugLog(
      `Typo correction test: Player is on ${isOnSubmittedScreen ? "submitted" : "typo check"} screen`,
    );

    // ============================================
    // TEST: Verify typo check UI elements exist
    // ============================================
    debugLog("Typo correction test: Verifying UI elements exist...");

    // Check that the typo check screen has all required elements (attached to DOM)
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

    // ============================================
    // TEST: If on typo check screen, verify the flow works
    // ============================================
    if (isOnTypoCheckScreen) {
      debugLog(
        "Typo correction test: LLM suggested changes, testing accept flow...",
      );

      // Verify original and corrected text are shown
      const originalText = await players[0]
        .locator("#originalText")
        .textContent();
      expect(originalText).toContain("Testantwort");

      // Click "Original behalten" to go back to submitted screen
      await players[0].click('button:has-text("Original behalten")');
      await players[0].waitForSelector("#submittedScreen.active", {
        timeout: 5000,
      });
    }

    debugLog("Typo correction test completed successfully!");
  });

  test("accepting typo correction does not trigger repeated prompts", async () => {
    const { host, players } = clients;

    // ============================================
    // SETUP: Get to writing phase with player
    // ============================================
    debugLog("Typo accept test: Setting up game...");

    await Promise.all([host.goto("/host"), players[0].goto("/player")]);

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
    await players[0].fill("#nameInput", "TypoAcceptTester");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Add prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Typo accept test question");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();

    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    await host.click("#startPromptSelectionBtn");
    await host.waitForTimeout(1000);
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Player submits answer with intentional typo
    // ============================================
    debugLog("Typo accept test: Player submitting answer with typo...");

    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    await players[0].fill(
      "#answerInput",
      "Das ist ein Test mit einm Tippfehler drin.",
    );
    await players[0].click("#submitButton");

    await players[0].waitForSelector("#submittedScreen.active", {
      timeout: 5000,
    });

    // Wait for typo check to complete
    await players[0].waitForTimeout(2500);

    const isOnTypoCheckScreen = await players[0]
      .locator("#typoCheckScreen.active")
      .isVisible();

    if (isOnTypoCheckScreen) {
      debugLog("Typo accept test: LLM suggested changes, accepting...");

      const correctedText = await players[0]
        .locator("#correctedText")
        .textContent();
      debugLog(`Typo accept test: Corrected text: ${correctedText}`);

      // Accept the correction
      await players[0].click('button:has-text("Korrektur übernehmen")');
      await players[0].waitForSelector("#submittedScreen.active", {
        timeout: 5000,
      });

      // Wait to ensure no repeated prompts appear
      await players[0].waitForTimeout(3000);

      // Verify we're still on submitted screen (not prompted again)
      await expect(players[0].locator("#submittedScreen")).toHaveClass(
        /active/,
      );
      const stillOnTypoCheck = await players[0]
        .locator("#typoCheckScreen.active")
        .isVisible();
      expect(stillOnTypoCheck).toBe(false);

      // Verify the corrected answer is on the host panel
      await host.click('.sidebar-item:has-text("Antworten")');
      await host.waitForSelector("#submissions.active");
      await host.waitForSelector(".submission-card", { timeout: 5000 });

      const submissionText = await host
        .locator(".submission-card .submission-text")
        .first()
        .textContent();
      expect(submissionText).toContain(correctedText || "");

      debugLog("Typo accept test: Verified no repeated prompts!");
    } else {
      debugLog(
        "Typo accept test: No LLM configured or no changes suggested, skipping accept flow test",
      );
    }

    debugLog("Typo accept test completed successfully!");
  });
});
