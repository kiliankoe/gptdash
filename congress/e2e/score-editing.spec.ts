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
 * Score editing tests
 *
 * Tests host functionality for manually editing player scores and clearing audience scores.
 */
test.describe("Score Editing", () => {
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

  test("host can edit player scores", async () => {
    test.setTimeout(60000);

    const { host, players, audience } = clients;

    // ============================================
    // SETUP: Create a game and play through to get scores
    // ============================================
    debugLog("Setting up game with players and audience...");

    await Promise.all([
      host.goto("/host"),
      players[0].goto("/player"),
      audience[0].goto("/"),
    ]);

    await waitForConnection(host);

    // Create player tokens
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");
    const tokens = await getPlayerTokens(host);

    // Player joins and registers
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "ScoreTestPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Audience joins
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active");

    // Add and queue prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Score test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Start round
    await host.click("#startPromptSelectionBtn");
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Player submits answer
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[0].fill("#answerInput", "Test answer for scoring");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Set manual AI answer
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "AI answer for score test");
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // Transition through REVEAL to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await expect(host.locator("#overviewPhase")).toHaveText("REVEAL", {
      timeout: 5000,
    });

    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("VOTING", {
      timeout: 5000,
    });

    // Audience votes
    await audience[0].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });
    await audience[0]
      .locator("#aiAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0]
      .locator("#funnyAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    // Transition to RESULTS
    await host.click('button[data-phase="RESULTS"]');
    await expect(host.locator("#overviewPhase")).toHaveText("RESULTS", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Edit player scores
    // ============================================
    debugLog("Testing player score editing...");

    // Navigate to scores panel
    await host.click('.sidebar-item:has-text("Punkte")');
    await host.waitForSelector("#scores.active");

    // Wait for scores to appear
    await host.waitForSelector("#playerScores .score-item", { timeout: 5000 });

    // Click on player score to open edit modal
    await host.click("#playerScores .score-item");

    // Wait for modal to appear
    await host.waitForSelector("#scoreEditModal", {
      state: "visible",
      timeout: 5000,
    });

    // Verify modal shows player name
    await expect(host.locator("#scoreEditPlayerName")).toHaveText(
      "ScoreTestPlayer",
    );

    // Edit the scores
    await host.fill("#scoreEditAiPoints", "10");
    await host.fill("#scoreEditFunnyPoints", "5");

    // Save the scores
    await host.click('#scoreEditModal button:has-text("Speichern")');

    // Wait for modal to close
    await host.waitForSelector("#scoreEditModal", {
      state: "hidden",
      timeout: 5000,
    });

    // Verify the new scores are displayed
    await host.waitForTimeout(500); // Wait for broadcast to propagate
    const playerScoreItem = host.locator("#playerScores .score-item").first();
    await expect(playerScoreItem).toContainText("15 Pkt"); // 10 + 5 = 15 total
    await expect(playerScoreItem).toContainText("AI: 10");
    await expect(playerScoreItem).toContainText("Funny: 5");

    debugLog("Player score editing test completed successfully!");
  });

  test("host can clear audience scores", async () => {
    test.setTimeout(60000);

    const { host, players, audience } = clients;

    // ============================================
    // SETUP: Create a game and play through to get audience scores
    // ============================================
    debugLog("Setting up game with audience scores...");

    await Promise.all([
      host.goto("/host"),
      players[0].goto("/player"),
      audience[0].goto("/"),
    ]);

    await waitForConnection(host);

    // Create player tokens
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForSelector("#playerTokensList .token");
    const tokens = await getPlayerTokens(host);

    // Player joins and registers
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "TestPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Audience joins
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active");

    // Get audience display name for later verification
    const audienceName =
      (await audience[0].locator("#headerDisplayName").textContent()) || "";
    debugLog(`Audience member name: ${audienceName}`);

    // Add and queue prompt
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Audience score test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Start round
    await host.click("#startPromptSelectionBtn");
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Player submits answer
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[0].fill("#answerInput", "Player answer");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Set manual AI answer
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.waitForSelector(".submission-card", { timeout: 5000 });
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "AI answer for audience score test");
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // Transition through REVEAL to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await expect(host.locator("#overviewPhase")).toHaveText("REVEAL", {
      timeout: 5000,
    });

    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await expect(host.locator("#overviewPhase")).toHaveText("VOTING", {
      timeout: 5000,
    });

    // Audience votes - vote for the player's answer as AI (which is correct)
    await audience[0].waitForSelector("#votingScreen.active", {
      timeout: 5000,
    });
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Vote for the answer that contains "Player answer" as AI
    const aiOptions = audience[0].locator("#aiAnswerOptions .answer-option");
    await aiOptions.first().click();
    await audience[0]
      .locator("#funnyAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active", {
      timeout: 5000,
    });

    // Transition to RESULTS
    await host.click('button[data-phase="RESULTS"]');
    await expect(host.locator("#overviewPhase")).toHaveText("RESULTS", {
      timeout: 5000,
    });

    // ============================================
    // TEST: Clear audience scores
    // ============================================
    debugLog("Testing audience score clearing...");

    // Navigate to scores panel
    await host.click('.sidebar-item:has-text("Punkte")');
    await host.waitForSelector("#scores.active");

    // Wait for audience scores to appear
    await host.waitForSelector("#audienceScores .score-item", {
      timeout: 5000,
    });

    // Verify audience member is in the scores
    const audienceScoresBefore = host.locator("#audienceScores .score-item");
    const countBefore = await audienceScoresBefore.count();
    expect(countBefore).toBeGreaterThan(0);

    // Handle confirmation dialog
    host.on("dialog", (dialog) => dialog.accept());

    // Click on the delete button to clear the audience score
    await host.click("#audienceScores .score-delete-btn");

    // Wait for the score to be removed
    await host.waitForTimeout(500);

    // Verify audience scores section is now empty or shows placeholder
    const audienceScoresAfter = host.locator("#audienceScores .score-item");
    const countAfter = await audienceScoresAfter.count();
    expect(countAfter).toBe(countBefore - 1);

    debugLog("Audience score clearing test completed successfully!");
  });

  test("score edit modal can be cancelled", async () => {
    test.setTimeout(60000);

    const { host, players, audience } = clients;

    // ============================================
    // SETUP: Create a game with scores
    // ============================================
    debugLog("Setting up game for modal cancel test...");

    await Promise.all([
      host.goto("/host"),
      players[0].goto("/player"),
      audience[0].goto("/"),
    ]);

    await waitForConnection(host);

    // Create player
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
    await players[0].fill("#nameInput", "CancelTestPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Audience joins
    if (await audience[0].locator("#joinButton").isVisible()) {
      await audience[0].click("#joinButton");
    }
    await audience[0].waitForSelector("#waitingScreen.active");

    // Quick game setup
    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");
    await host.fill("#promptText", "Cancel test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
    });
    await host.click("#startPromptSelectionBtn");
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Player submits
    await players[0].waitForSelector("#writingScreen.active");
    await players[0].fill("#answerInput", "Cancel test answer");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Set AI and complete round
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector(".submission-card", { timeout: 5000 });
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "AI cancel test");
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card");
    await host.locator(".ai-submission-card").first().click();

    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');

    // Audience votes
    await audience[0].waitForSelector("#votingScreen.active");
    await audience[0].waitForSelector(".answer-option");
    await audience[0]
      .locator("#aiAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0]
      .locator("#funnyAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active");

    await host.click('button[data-phase="RESULTS"]');

    // ============================================
    // TEST: Open modal and cancel
    // ============================================
    debugLog("Testing modal cancel...");

    await host.click('.sidebar-item:has-text("Punkte")');
    await host.waitForSelector("#scores.active");
    await host.waitForSelector("#playerScores .score-item", { timeout: 5000 });

    // Get original score
    const originalScoreText = await host
      .locator("#playerScores .score-item .score-total")
      .first()
      .textContent();

    // Open modal
    await host.click("#playerScores .score-item");
    await host.waitForSelector("#scoreEditModal", { state: "visible" });

    // Change values but cancel
    await host.fill("#scoreEditAiPoints", "999");
    await host.fill("#scoreEditFunnyPoints", "999");

    // Click cancel button
    await host.click('#scoreEditModal button:has-text("Abbrechen")');

    // Verify modal closed
    await host.waitForSelector("#scoreEditModal", { state: "hidden" });

    // Verify score was NOT changed
    const newScoreText = await host
      .locator("#playerScores .score-item .score-total")
      .first()
      .textContent();
    expect(newScoreText).toBe(originalScoreText);

    debugLog("Modal cancel test completed successfully!");
  });
});
