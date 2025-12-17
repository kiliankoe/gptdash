import { test, expect, type BrowserContext } from "@playwright/test";
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

  test("host can add trivia question with 3 choices", async () => {
    const { host } = clients;

    await host.goto("/host");
    await waitForConnection(host);

    // Navigate to Trivia panel
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");

    // Fill in question
    await host.fill(
      "#triviaQuestionText",
      "Was ist die Hauptstadt von Deutschland?",
    );

    // Fill in choices and mark B as correct
    await host.fill("#triviaChoice0", "Paris");
    await host.fill("#triviaChoice1", "Berlin");
    await host.fill("#triviaChoice2", "Rom");
    await host.click("#triviaCorrect1"); // Mark B as correct

    // Add question
    await host.click('button:has-text("Frage hinzufuegen")');

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

    // Add trivia question
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await host.fill("#triviaQuestionText", "Test trivia during writing?");
    await host.fill("#triviaChoice0", "Option A");
    await host.fill("#triviaChoice1", "Option B");
    await host.fill("#triviaChoice2", "Option C");
    await host.click("#triviaCorrect0");
    await host.click('button:has-text("Frage hinzufuegen")');
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

    // Add and present trivia
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await host.fill("#triviaQuestionText", "Audience vote test?");
    await host.fill("#triviaChoice0", "Answer A");
    await host.fill("#triviaChoice1", "Answer B");
    await host.fill("#triviaChoice2", "Answer C");
    await host.click("#triviaCorrect1");
    await host.click('button:has-text("Frage hinzufuegen")');
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

    // Add and present trivia
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await host.fill("#triviaQuestionText", "Resolve test question?");
    await host.fill("#triviaChoice0", "Wrong A");
    await host.fill("#triviaChoice1", "Correct B");
    await host.fill("#triviaChoice2", "Wrong C");
    await host.click("#triviaCorrect1"); // B is correct
    await host.click('button:has-text("Frage hinzufuegen")');
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
    await expect(
      beamer.locator(".trivia-result-choice.correct"),
    ).toContainText("Correct B");

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

    // Add and present trivia
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await host.fill("#triviaQuestionText", "Phase change clear test?");
    await host.fill("#triviaChoice0", "A");
    await host.fill("#triviaChoice1", "B");
    await host.fill("#triviaChoice2", "C");
    await host.click("#triviaCorrect0");
    await host.click('button:has-text("Frage hinzufuegen")');
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
    await expect(beamer.locator("#triviaOverlay")).toBeHidden({ timeout: 5000 });
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

    // Add and present trivia
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await host.fill("#triviaQuestionText", "Clear test?");
    await host.fill("#triviaChoice0", "A");
    await host.fill("#triviaChoice1", "B");
    await host.fill("#triviaChoice2", "C");
    await host.click("#triviaCorrect0");
    await host.click('button:has-text("Frage hinzufuegen")');
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
    await expect(beamer.locator("#triviaOverlay")).toBeHidden({ timeout: 5000 });
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

    // Add question
    await host.fill("#triviaQuestionText", "Question to delete?");
    await host.fill("#triviaChoice0", "A");
    await host.fill("#triviaChoice1", "B");
    await host.fill("#triviaChoice2", "C");
    await host.click("#triviaCorrect0");
    await host.click('button:has-text("Frage hinzufuegen")');
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

    // Add and present trivia
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await host.fill("#triviaQuestionText", "Reconnect test question?");
    await host.fill("#triviaChoice0", "Alpha");
    await host.fill("#triviaChoice1", "Beta");
    await host.fill("#triviaChoice2", "Gamma");
    await host.click("#triviaCorrect2");
    await host.click('button:has-text("Frage hinzufuegen")');
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

    // Add trivia question
    await host.click('.sidebar-item:has-text("Trivia")');
    await host.waitForSelector("#trivia.active");
    await host.fill("#triviaQuestionText", "Export persistence test?");
    await host.fill("#triviaChoice0", "Export A");
    await host.fill("#triviaChoice1", "Export B");
    await host.fill("#triviaChoice2", "Export C");
    await host.click("#triviaCorrect1");
    await host.click('button:has-text("Frage hinzufuegen")');
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
