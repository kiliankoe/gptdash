import { expect, test, type BrowserContext } from "@playwright/test";
import {
  closeContexts,
  createGameClients,
  getPlayerTokens,
  resetGameState,
  waitForBeamerScene,
  waitForConnection,
  type GameClients,
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

  test("results show 'KI richtig erkannt!' when audience identifies AI correctly", async () => {
    test.setTimeout(60000);

    const { host, beamer, players, audience } = clients;

    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer"),
      players[0].goto("/player"),
      audience[0].goto("/"),
    ]);

    await Promise.all([waitForConnection(host), waitForConnection(beamer)]);

    // Create player and get token
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
    await players[0].fill("#nameInput", "TestPlayer");
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
    await host.fill("#promptText", "AI detection test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
    });
    await host.click("#startPromptSelectionBtn");
    await waitForBeamerScene(beamer, "sceneWriting");

    // Player submits answer
    await players[0].waitForSelector("#answerInput", { timeout: 10000 });
    await players[0].fill("#answerInput", "Player answer for AI test");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Set manual AI answer
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "AI answer for detection test");
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // Transition to REVEAL and reveal answers
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await waitForBeamerScene(beamer, "sceneReveal");
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // Transition to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await waitForBeamerScene(beamer, "sceneVoting");

    // Audience votes for the AI answer (find option containing AI text)
    await audience[0].waitForSelector("#votingScreen.active");
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Click on the AI answer option for AI vote
    const aiOption = audience[0].locator(
      '#aiAnswerOptions .answer-option:has-text("AI answer for detection test")',
    );
    await aiOption.click();
    // Click any option for funny vote
    await audience[0]
      .locator("#funnyAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active");

    // Transition to RESULTS (starts at breakdown step)
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="RESULTS"]');
    await waitForBeamerScene(beamer, "sceneResultsBreakdown");

    // Advance to leaderboards step where AI reveal is shown
    await host.click('.sidebar-item:has-text("bersicht")');
    await host.click("#overviewPrimaryActionBtn"); // "Leaderboards zeigen"
    await waitForBeamerScene(beamer, "sceneResultsLeaderboards");
    // Wait for scores message to arrive with ai_submission_id
    await beamer.waitForTimeout(500);

    // Verify the AI label shows "KI richtig erkannt!"
    const aiLabel = beamer.locator("#aiRevealLabel");
    await expect(aiLabel).toHaveText("KI richtig erkannt!");
  });

  test("results show 'Beste KI-Imitation' when player fools audience", async () => {
    test.setTimeout(60000);

    const { host, beamer, players, audience } = clients;

    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer"),
      players[0].goto("/player"),
      audience[0].goto("/"),
    ]);

    await Promise.all([waitForConnection(host), waitForConnection(beamer)]);

    // Create player and get token
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
    await players[0].fill("#nameInput", "Trickster");
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
    await host.fill("#promptText", "Fool the audience test");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
    });
    await host.click("#startPromptSelectionBtn");
    await waitForBeamerScene(beamer, "sceneWriting");

    // Player submits a convincing AI-like answer
    await players[0].waitForSelector("#answerInput", { timeout: 10000 });
    await players[0].fill(
      "#answerInput",
      "Player fooling audience with AI-style answer",
    );
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Set manual AI answer
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "Actual AI answer here");
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // Transition to REVEAL and reveal answers
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await waitForBeamerScene(beamer, "sceneReveal");
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // Transition to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await waitForBeamerScene(beamer, "sceneVoting");

    // Audience votes for the PLAYER answer (thinking it's AI)
    await audience[0].waitForSelector("#votingScreen.active");
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Find and vote for the player's answer (contains "Player fooling") as AI
    await audience[0]
      .locator(
        '#aiAnswerOptions .answer-option:has(.text:text("Player fooling"))',
      )
      .click();
    // Vote for any answer as funny
    await audience[0]
      .locator("#funnyAnswerOptions .answer-option")
      .first()
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active");

    // Transition to RESULTS (starts at breakdown step)
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="RESULTS"]');
    await waitForBeamerScene(beamer, "sceneResultsBreakdown");

    // Advance to leaderboards step where AI reveal is shown
    await host.click('.sidebar-item:has-text("bersicht")');
    await host.click("#overviewPrimaryActionBtn"); // "Leaderboards zeigen"
    await waitForBeamerScene(beamer, "sceneResultsLeaderboards");
    // Wait for scores message to arrive with ai_submission_id
    await beamer.waitForTimeout(500);

    // Since audience voted for player's answer as AI (but it's not the real AI),
    // label should show "Beste KI-Imitation"
    const aiLabel = beamer.locator("#aiRevealLabel");
    await expect(aiLabel).toHaveText("Beste KI-Imitation");
  });

  test("results show 'Die KI war am lustigsten?!' when AI wins funny vote", async () => {
    test.setTimeout(60000);

    const { host, beamer, players, audience } = clients;

    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer"),
      players[0].goto("/player"),
      audience[0].goto("/"),
    ]);

    await Promise.all([waitForConnection(host), waitForConnection(beamer)]);

    // Create player and get token
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
    await players[0].fill("#nameInput", "NotFunny");
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
    await host.fill("#promptText", "Funny AI test prompt");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
    });
    await host.click("#startPromptSelectionBtn");
    await waitForBeamerScene(beamer, "sceneWriting");

    // Player submits a boring answer
    await players[0].waitForSelector("#answerInput", { timeout: 10000 });
    await players[0].fill("#answerInput", "Boring player answer");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Set a hilarious AI answer
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "Hilarious AI comedy gold answer");
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // Transition to REVEAL and reveal answers
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await waitForBeamerScene(beamer, "sceneReveal");
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // Transition to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await waitForBeamerScene(beamer, "sceneVoting");

    // Audience votes for AI answer as funniest
    await audience[0].waitForSelector("#votingScreen.active");
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Click any for AI vote
    await audience[0]
      .locator("#aiAnswerOptions .answer-option")
      .first()
      .click();
    // Click on the AI answer for funny vote (matches "Hilarious AI" from "Hilarious AI comedy gold answer")
    await audience[0]
      .locator(
        '#funnyAnswerOptions .answer-option:has(.text:text("Hilarious AI"))',
      )
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active");

    // Transition to RESULTS (starts at breakdown step)
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="RESULTS"]');
    await waitForBeamerScene(beamer, "sceneResultsBreakdown");

    // Advance to leaderboards step where AI reveal is shown
    await host.click('.sidebar-item:has-text("bersicht")');
    await host.click("#overviewPrimaryActionBtn"); // "Leaderboards zeigen"
    await waitForBeamerScene(beamer, "sceneResultsLeaderboards");
    // Wait for scores message to arrive with ai_submission_id
    await beamer.waitForTimeout(500);

    // Verify the funny label shows "Die KI war am lustigsten?!"
    const funnyLabel = beamer.locator("#funnyRevealLabel");
    await expect(funnyLabel).toHaveText("Die KI war am lustigsten?!");
  });

  test("results show 'Am lustigsten' when player wins funny vote", async () => {
    test.setTimeout(60000);

    const { host, beamer, players, audience } = clients;

    await Promise.all([
      host.goto("/host"),
      beamer.goto("/beamer"),
      players[0].goto("/player"),
      audience[0].goto("/"),
    ]);

    await Promise.all([waitForConnection(host), waitForConnection(beamer)]);

    // Create player and get token
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
    await players[0].fill("#nameInput", "Comedian");
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
    await host.fill("#promptText", "Player comedy test");
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList [data-prompt-id]");
    await host.locator("#hostPromptsList .queue-btn").first().click();
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
    });
    await host.click("#startPromptSelectionBtn");
    await waitForBeamerScene(beamer, "sceneWriting");

    // Player submits a hilarious answer
    await players[0].waitForSelector("#answerInput", { timeout: 10000 });
    await players[0].fill("#answerInput", "Super funny player joke answer");
    await players[0].click("#submitButton");
    await players[0].waitForSelector("#submittedScreen.active");

    // Set a boring AI answer
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");
    await host.click('summary:has-text("Manuelle KI-Antwort")');
    await host.waitForSelector("#manualAiText", { state: "visible" });
    await host.fill("#manualAiText", "Boring AI answer");
    await host.click('button:has-text("Als KI-Antwort speichern")');
    await host.waitForSelector(".ai-submission-card", { timeout: 5000 });
    await host.locator(".ai-submission-card").first().click();
    await host.waitForTimeout(300);

    // Transition to REVEAL and reveal answers
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="REVEAL"]');
    await waitForBeamerScene(beamer, "sceneReveal");
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);
    await host.click('#submissions button:has-text("Weiter")');
    await host.waitForTimeout(500);

    // Transition to VOTING
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="VOTING"]');
    await waitForBeamerScene(beamer, "sceneVoting");

    // Audience votes for player answer as funniest
    await audience[0].waitForSelector("#votingScreen.active");
    await audience[0].waitForSelector(".answer-option", { timeout: 5000 });

    // Click any for AI vote
    await audience[0]
      .locator("#aiAnswerOptions .answer-option")
      .first()
      .click();
    // Vote for the player's funny answer (contains "Super funny")
    await audience[0]
      .locator(
        '#funnyAnswerOptions .answer-option:has(.text:text("Super funny"))',
      )
      .click();
    await audience[0].click("#voteButton");
    await audience[0].waitForSelector("#confirmedScreen.active");

    // Transition to RESULTS (starts at breakdown step)
    await host.click('.sidebar-item:has-text("Spiel-Steuerung")');
    await host.click('button[data-phase="RESULTS"]');
    await waitForBeamerScene(beamer, "sceneResultsBreakdown");

    // Advance to leaderboards step where AI reveal is shown
    await host.click('.sidebar-item:has-text("bersicht")');
    await host.click("#overviewPrimaryActionBtn"); // "Leaderboards zeigen"
    await waitForBeamerScene(beamer, "sceneResultsLeaderboards");
    // Wait for scores message to arrive with ai_submission_id
    await beamer.waitForTimeout(500);

    // Verify the funny label shows "Am lustigsten"
    const funnyLabel = beamer.locator("#funnyRevealLabel");
    await expect(funnyLabel).toHaveText("Am lustigsten");
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
    await players[0].waitForSelector("#registerScreen.active", {
      timeout: 5000,
    });
    await players[0].fill("#nameInput", "TestPlayer");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active", {
      timeout: 5000,
    });

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
    await players[0].waitForSelector("#submittedScreen.active", {
      timeout: 5000,
    });

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
