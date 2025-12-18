import { test, expect, type BrowserContext, type Page } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  waitForBeamerScene,
  resetGameState,
  createGameClients,
  closeContexts,
  setupGameToWriting,
} from "./test-utils";

/**
 * Helper function to add a trivia question with any number of choices (2-4)
 */
async function addTriviaQuestion(
  host: Page,
  question: string,
  choices: string[],
  correctIndex: number,
): Promise<void> {
  await host.fill("#triviaQuestionText", question);

  // Add extra choice buttons if needed (UI starts with 2 choices)
  const extraChoices = choices.length - 2;
  for (let i = 0; i < extraChoices; i++) {
    await host.click("#addTriviaChoiceBtn");
    // Wait for the new choice field to appear
    await host.waitForSelector(`#triviaChoice${2 + i}`, { timeout: 5000 });
  }

  // Fill in choices
  for (let i = 0; i < choices.length; i++) {
    await host.fill(`#triviaChoice${i}`, choices[i]);
  }

  // Mark correct answer
  await host.click(`#triviaCorrect${correctIndex}`);

  // Submit
  await host.click('button:has-text("Frage hinzufuegen")');
}

/**
 * Trivia feature tests
 *
 * Tests trivia functionality during WRITING phase:
 * - Host can add/present/resolve/remove trivia questions
 * - Audience can vote on trivia
 * - Beamer displays trivia
 */
test.describe("Trivia", () => {
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

  test("host can add trivia question with 2 choices (default)", async () => {
    const { host } = clients;

    await host.goto("/host");
    await waitForConnection(host);

    // Navigate to Trivia panel
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");

    // Add trivia with 2 choices (default, no add button needed)
    await addTriviaQuestion(
      host,
      "Was ist die Hauptstadt von Deutschland?",
      ["Paris", "Berlin"],
      1, // Berlin is correct
    );

    // Verify question appears in list
    await expect(
      host.locator(
        ".trivia-question-card:has-text('Was ist die Hauptstadt von Deutschland?')",
      ),
    ).toBeVisible({ timeout: 5000 });

    // Verify correct answer is highlighted
    await expect(
      host.locator(".trivia-question-card:has-text('Berlin')"),
    ).toBeVisible();

    // Verify question count is updated
    await expect(host.locator("#triviaQuestionCount")).toHaveText("1");
  });

  test("host can present trivia during WRITING phase", async () => {
    const { host, beamer, players } = clients;

    await host.goto("/host");
    await beamer.goto("/beamer.html");
    await waitForConnection(host);

    // Setup game to WRITING phase
    await setupGameToWriting(host, players);
    await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
      timeout: 5000,
    });

    // Add trivia question with 3 choices
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await addTriviaQuestion(
      host,
      "Test trivia during writing?",
      ["Option A", "Option B", "Option C"],
      0, // Option A is correct
    );
    await host.waitForSelector(".trivia-question-card");

    // Present trivia
    await host.click('.trivia-question-card button:has-text("Praesentieren")');

    // Verify active trivia card shows
    await expect(host.locator("#activeTriviaCard")).toBeVisible({
      timeout: 5000,
    });
    await expect(host.locator("#activeTriviaText")).toHaveText(
      "Test trivia during writing?",
    );

    // Verify beamer shows trivia overlay
    await expect(beamer.locator("#triviaOverlay")).toBeVisible({
      timeout: 5000,
    });
    await expect(beamer.locator("#triviaQuestion")).toHaveText(
      "Test trivia during writing?",
    );
  });

  test("audience can vote on trivia question", async () => {
    const { host, audience, players } = clients;

    await host.goto("/host");
    await waitForConnection(host);

    // Setup game to WRITING phase
    await setupGameToWriting(host, players);

    // Add and present trivia with 4 choices (max)
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await addTriviaQuestion(
      host,
      "Audience vote test?",
      ["Answer A", "Answer B", "Answer C", "Answer D"],
      1, // Answer B is correct
    );
    await host.waitForSelector(".trivia-question-card");
    await host.click('.trivia-question-card button:has-text("Praesentieren")');
    await expect(host.locator("#activeTriviaCard")).toBeVisible({
      timeout: 5000,
    });

    // Audience joins and sees trivia
    await audience[0].goto("/");
    await waitForConnection(audience[0]);

    // Check if trivia screen is already showing (auto-shown when trivia is active during WRITING)
    // or if we need to click the join button first
    const triviaAlreadyShowing = await audience[0]
      .locator("#triviaScreen.active")
      .isVisible()
      .catch(() => false);

    if (!triviaAlreadyShowing) {
      // If join button is visible, click it
      const joinButtonVisible = await audience[0]
        .locator("#joinButton")
        .isVisible()
        .catch(() => false);
      if (joinButtonVisible) {
        await audience[0].click("#joinButton");
      }
    }

    await audience[0].waitForSelector("#triviaScreen.active", {
      timeout: 5000,
    });

    // Verify trivia question is shown
    await expect(audience[0].locator("#triviaQuestionText")).toHaveText(
      "Audience vote test?",
    );

    // Click to vote for choice B
    await audience[0].click('.trivia-option:has-text("Answer B")');

    // Should show confirmed screen
    await audience[0].waitForSelector("#triviaConfirmedScreen.active", {
      timeout: 5000,
    });
    await expect(audience[0].locator("#triviaVoteSummary")).toContainText(
      "Answer B",
    );

    // Verify vote count increased on host
    await expect(host.locator("#activeTriviaVoteCount")).toHaveText("1", {
      timeout: 5000,
    });
  });

  test("host can resolve trivia and see results", async () => {
    const { host, beamer, audience, players } = clients;

    await host.goto("/host");
    await beamer.goto("/beamer.html");
    await waitForConnection(host);

    // Setup game to WRITING phase
    await setupGameToWriting(host, players);

    // Add and present trivia with 3 choices
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await addTriviaQuestion(
      host,
      "Resolve test question?",
      ["Wrong A", "Correct B", "Wrong C"],
      1, // B is correct
    );
    await host.waitForSelector(".trivia-question-card");
    await host.click('.trivia-question-card button:has-text("Praesentieren")');
    await expect(host.locator("#activeTriviaCard")).toBeVisible();

    // Audience votes
    await audience[0].goto("/");
    await waitForConnection(audience[0]);

    // Check if trivia screen is already showing (auto-shown when trivia is active during WRITING)
    const triviaAlreadyShowing = await audience[0]
      .locator("#triviaScreen.active")
      .isVisible()
      .catch(() => false);

    if (!triviaAlreadyShowing) {
      const joinButtonVisible = await audience[0]
        .locator("#joinButton")
        .isVisible()
        .catch(() => false);
      if (joinButtonVisible) {
        await audience[0].click("#joinButton");
      }
    }

    await audience[0].waitForSelector("#triviaScreen.active", {
      timeout: 5000,
    });
    await audience[0].click('.trivia-option:has-text("Correct B")');
    await audience[0].waitForSelector("#triviaConfirmedScreen.active");

    // Host resolves trivia
    await host.click('button:has-text("Aufloesen")');

    // Active trivia card should hide
    await expect(host.locator("#activeTriviaCard")).toBeHidden({
      timeout: 5000,
    });

    // Beamer should show result overlay with correct answer highlighted
    await expect(beamer.locator("#triviaResultOverlay")).toBeVisible({
      timeout: 5000,
    });
    await expect(beamer.locator(".trivia-result-choice.correct")).toContainText(
      "Correct B",
    );

    // Audience should see result screen
    await expect(audience[0].locator("#triviaResultScreen.active")).toBeVisible(
      { timeout: 5000 },
    );
  });

  test("trivia clears when leaving WRITING phase", async () => {
    const { host, beamer, players } = clients;

    await host.goto("/host");
    await beamer.goto("/beamer.html");
    await waitForConnection(host);

    // Setup game to WRITING phase with player submission
    await setupGameToWriting(host, players);

    // Wait for WRITING phase to be active
    await expect(host.locator("#overviewPhase")).toContainText("WRITING", {
      timeout: 10000,
    });

    // Player submits answer
    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });
    await players[0].fill("#answerInput", "My answer for the trivia test");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Add and present trivia with 2 choices (simplest case)
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await addTriviaQuestion(host, "Phase change clear test?", ["A", "B"], 0);
    await host.waitForSelector(".trivia-question-card");
    await host.click('.trivia-question-card button:has-text("Praesentieren")');
    await expect(beamer.locator("#triviaOverlay")).toBeVisible({
      timeout: 5000,
    });

    // Advance to REVEAL phase
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');

    // Wait for phase change
    await expect(host.locator("#overviewPhase")).toContainText("REVEAL", {
      timeout: 5000,
    });

    // Trivia overlay should be cleared
    await expect(beamer.locator("#triviaOverlay")).toBeHidden({
      timeout: 5000,
    });
    await expect(beamer.locator("#triviaResultOverlay")).toBeHidden();

    // Beamer should show reveal scene
    await waitForBeamerScene(beamer, "sceneReveal", 5000);
  });

  test("host can clear trivia without showing results", async () => {
    const { host, beamer, players } = clients;

    await host.goto("/host");
    await beamer.goto("/beamer.html");
    await waitForConnection(host);

    // Setup game to WRITING phase
    await setupGameToWriting(host, players);

    // Add and present trivia with 2 choices (simplest case)
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await addTriviaQuestion(host, "Clear test?", ["A", "B"], 0);
    await host.waitForSelector(".trivia-question-card");
    await host.click('.trivia-question-card button:has-text("Praesentieren")');
    await expect(beamer.locator("#triviaOverlay")).toBeVisible({
      timeout: 5000,
    });

    // Host clears trivia without resolving
    await host.click('button:has-text("Ausblenden")');

    // Active trivia card should hide
    await expect(host.locator("#activeTriviaCard")).toBeHidden({
      timeout: 5000,
    });

    // Both overlays should be hidden (no result shown)
    await expect(beamer.locator("#triviaOverlay")).toBeHidden({
      timeout: 5000,
    });
    await expect(beamer.locator("#triviaResultOverlay")).toBeHidden();

    // Beamer should still be in WRITING scene (not showing results)
    await waitForBeamerScene(beamer, "sceneWriting", 5000);
  });

  test("host can remove trivia question", async () => {
    const { host } = clients;

    await host.goto("/host");
    await waitForConnection(host);

    // Navigate to Trivia panel
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");

    // Add question with 3 choices
    await addTriviaQuestion(host, "Question to delete?", ["A", "B", "C"], 0);
    await host.waitForSelector(".trivia-question-card");

    // Verify question is there
    await expect(host.locator("#triviaQuestionCount")).toHaveText("1");

    // Handle confirmation dialog
    host.on("dialog", (dialog) => dialog.accept());

    // Remove question
    await host.click('.trivia-question-card button:has-text("Loeschen")');

    // Question should be removed
    await expect(host.locator("#triviaQuestionCount")).toHaveText("0", {
      timeout: 5000,
    });
    await expect(
      host.locator(".trivia-question-card:has-text('Question to delete?')"),
    ).toBeHidden();
  });

  test("audience reconnecting sees active trivia", async () => {
    const { host, audience, players } = clients;

    await host.goto("/host");
    await waitForConnection(host);

    // Setup game to WRITING phase
    await setupGameToWriting(host, players);

    // Add and present trivia with 4 choices (max)
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await addTriviaQuestion(
      host,
      "Reconnect test question?",
      ["Alpha", "Beta", "Gamma", "Delta"],
      2, // Gamma is correct
    );
    await host.waitForSelector(".trivia-question-card");
    await host.click('.trivia-question-card button:has-text("Praesentieren")');
    await expect(host.locator("#activeTriviaCard")).toBeVisible();

    // Audience joins for the first time (should see trivia)
    await audience[0].goto("/");
    await waitForConnection(audience[0]);

    // Check if trivia screen is already showing (auto-shown when trivia is active during WRITING)
    const triviaAlreadyShowing = await audience[0]
      .locator("#triviaScreen.active")
      .isVisible()
      .catch(() => false);

    if (!triviaAlreadyShowing) {
      const joinButtonVisible = await audience[0]
        .locator("#joinButton")
        .isVisible()
        .catch(() => false);
      if (joinButtonVisible) {
        await audience[0].click("#joinButton");
      }
    }

    await audience[0].waitForSelector("#triviaScreen.active", {
      timeout: 5000,
    });

    // Verify trivia question is shown
    await expect(audience[0].locator("#triviaQuestionText")).toHaveText(
      "Reconnect test question?",
    );
    await expect(
      audience[0].locator('.trivia-option:has-text("Gamma")'),
    ).toBeVisible();
  });

  test("trivia questions persist in state export", async () => {
    const { host } = clients;

    await host.goto("/host");
    await waitForConnection(host);

    // Add trivia question with 3 choices
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await addTriviaQuestion(
      host,
      "Export persistence test?",
      ["Export A", "Export B", "Export C"],
      1, // Export B is correct
    );
    await host.waitForSelector(".trivia-question-card");

    // Navigate to State Export panel
    await host.click('.sidebar-item:has-text("State-Export")');
    await host.waitForSelector("#state.active");

    // Click refresh to load state
    await host.click('#state button:has-text("Aktualisieren")');
    await host.waitForTimeout(500);

    // Get the state JSON
    const stateText = await host.locator("#stateJsonView").textContent();
    expect(stateText).toContain("trivia_questions");
    expect(stateText).toContain("Export persistence test?");
    expect(stateText).toContain("Export A");
    expect(stateText).toContain("Export B");
    expect(stateText).toContain("Export C");
  });
});
