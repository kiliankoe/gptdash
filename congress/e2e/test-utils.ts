import type { Page, BrowserContext, Browser } from "@playwright/test";

/**
 * Shared test utilities and types for GPTDash e2e tests
 */

export interface GameClients {
  host: Page;
  beamer: Page;
  players: Page[];
  audience: Page[];
}

// Helper to wait for WebSocket connection
export async function waitForConnection(
  page: Page,
  timeout = 10000,
): Promise<void> {
  await page.waitForFunction(
    () => {
      const dot = document.getElementById("statusDot");
      return dot?.classList.contains("connected");
    },
    { timeout },
  );
}

// Helper to get text content safely
export async function getText(page: Page, selector: string): Promise<string> {
  const element = await page.$(selector);
  return element ? ((await element.textContent()) ?? "") : "";
}

// Helper to wait for phase on beamer
export async function waitForBeamerScene(
  beamer: Page,
  sceneId: string,
  timeout = 10000,
): Promise<void> {
  await beamer.waitForSelector(`#${sceneId}.active`, { timeout });
}

// Helper to extract player tokens from host UI
export async function getPlayerTokens(host: Page): Promise<string[]> {
  // Try new player-status-card format first, fall back to old token-display format
  const tokens = await host.$$eval(
    "#playerTokensList .player-token .token, #playerTokensList .token-display .token",
    (els) => els.map((el) => el.textContent?.trim() ?? ""),
  );
  return tokens;
}

// Helper to reset game state before tests
export async function resetGameState(browser: Browser): Promise<void> {
  const resetContext = await browser.newContext();
  const resetPage = await resetContext.newPage();
  await resetPage.goto("/host");
  await waitForConnection(resetPage);

  // Handle reset confirmation dialog
  resetPage.on("dialog", (dialog) => dialog.accept());
  await resetPage.click('.sidebar-item:has-text("Spiel-Steuerung")');
  await resetPage.waitForSelector("#game.active");
  // Reset game state (players, rounds, scores)
  await resetPage.click('button:has-text("Spiel zurücksetzen")');
  await resetPage.waitForTimeout(300);
  // Also clear the prompt pool for test isolation
  await resetPage.click('button:has-text("Prompt-Pool leeren")');
  await resetPage.waitForTimeout(300);
  await resetContext.close();
}

// Helper to create game clients with isolated contexts
export async function createGameClients(
  browser: Browser,
): Promise<{ clients: GameClients; contexts: BrowserContext[] }> {
  const hostContext = await browser.newContext();
  const beamerContext = await browser.newContext();
  const player1Context = await browser.newContext();
  const player2Context = await browser.newContext();
  const audience1Context = await browser.newContext();
  const audience2Context = await browser.newContext();

  const contexts = [
    hostContext,
    beamerContext,
    player1Context,
    player2Context,
    audience1Context,
    audience2Context,
  ];

  const hostPage = await hostContext.newPage();

  // Capture console logs from host page for debugging
  hostPage.on("console", (msg) => {
    if (
      msg.text().includes("host_submissions") ||
      msg.text().includes("Unhandled") ||
      msg.text().includes("updateSubmissionsList") ||
      msg.text().includes("submissionsList") ||
      msg.text().includes("game_state") ||
      msg.text().includes("Rendering") ||
      msg.text().includes("Creating card")
    ) {
      console.log(`[HOST CONSOLE] ${msg.type()}: ${msg.text()}`);
    }
  });

  const clients: GameClients = {
    host: hostPage,
    beamer: await beamerContext.newPage(),
    players: [await player1Context.newPage(), await player2Context.newPage()],
    audience: [
      await audience1Context.newPage(),
      await audience2Context.newPage(),
    ],
  };

  return { clients, contexts };
}

// Helper to close all contexts
export async function closeContexts(contexts: BrowserContext[]): Promise<void> {
  for (const ctx of contexts) {
    await ctx.close();
  }
}

// Helper to setup a basic game with players and prompt ready for WRITING
export async function setupGameToWriting(
  host: Page,
  players: Page[],
  playerNames: string[] = ["Alice", "Bob"],
): Promise<string[]> {
  // Navigate to Players panel and create tokens
  await host.click('.sidebar-item:has-text("Spieler")');
  await host.waitForSelector("#players.active");
  await host.fill("#playerCount", String(playerNames.length));
  await host.click('#players button:has-text("Spieler erstellen")');
  await host.waitForSelector("#playerTokensList .token");
  const tokens = await getPlayerTokens(host);

  // Players join and register
  for (let i = 0; i < Math.min(playerNames.length, players.length); i++) {
    await players[i].goto("/player.html");
    await players[i].fill("#tokenInput", tokens[i]);
    await players[i].click("#joinButton");
    await players[i].waitForSelector("#registerScreen.active");
    await players[i].fill("#nameInput", playerNames[i]);
    await players[i].click("#registerButton");
    await players[i].waitForSelector("#waitingScreen.active");
  }

  // Add prompt to pool
  await host.click('.sidebar-item:has-text("Prompts")');
  await host.waitForSelector("#prompts.active");
  await host.fill("#promptText", "Test prompt question");
  await host.click('#prompts button:has-text("Prompt hinzufügen")');
  await host.waitForSelector("#hostPromptsList .prompt-row");

  // Queue the prompt
  await host.locator("#hostPromptsList .prompt-row .queue-btn").first().click();
  await host.waitForSelector("#startPromptSelectionBtn", {
    state: "visible",
    timeout: 5000,
  });

  // Start prompt selection (auto-advances to WRITING with 1 prompt)
  await host.click("#startPromptSelectionBtn");
  await host.waitForTimeout(1000);

  return tokens;
}
