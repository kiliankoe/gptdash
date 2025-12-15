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
 * Multimodal prompt tests
 *
 * Tests prompts with images
 */
test.describe("Multimodal Prompts", () => {
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

  test("prompt with image displays on beamer and player screens", async () => {
    const { host, beamer, players } = clients;

    // Setup: Navigate to host page (game already reset by beforeEach)
    await host.goto("/host");
    await waitForConnection(host);

    // Create player
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForTimeout(500);

    const tokens = await getPlayerTokens(host);
    expect(tokens).toHaveLength(1);

    // Register player
    await players[0].goto("/player.html");
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "ImageTester");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Connect beamer
    await beamer.goto("/beamer.html");
    await waitForConnection(beamer);

    // ============================================
    // TEST: Add multimodal prompt with image URL
    // ============================================
    console.log("Multimodal prompt test: Adding prompt with image...");

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");

    // Fill in both text and image URL (use a local static asset so tests don't
    // depend on external network access).
    const imageUrl = "/img/manekineko.gif";
    await host.fill("#promptText", "Was siehst du auf diesem Bild?");

    // Expand the multimodal image details section
    await host.click('summary:has-text("Bild hinzufügen")');
    await host.waitForSelector("#promptImageUrl", { state: "visible" });
    await host.fill("#promptImageUrl", imageUrl);

    // Add prompt to pool and queue it
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList .prompt-row");
    await host.locator("#hostPromptsList .prompt-row .queue-btn").first().click();

    // Wait for start button to become visible (triggered by server response)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Start prompt selection (auto-advances to WRITING with 1 prompt)
    await host.click("#startPromptSelectionBtn");
    await host.waitForTimeout(1000);

    // ============================================
    // VERIFY: Beamer shows the image in writing scene
    // ============================================
    console.log("Multimodal prompt test: Verifying beamer displays image...");

    await waitForBeamerScene(beamer, "sceneWriting", 10000);

    // Check that the prompt image is visible
    const beamerImage = beamer.locator("#writingPromptImage img");
    await expect(beamerImage).toBeVisible({ timeout: 5000 });

    // Verify the image URL is correct
    const beamerImageSrc = await beamerImage.getAttribute("src");
    expect(beamerImageSrc).toBe(imageUrl);

    // Verify text prompt is also shown
    const beamerPromptText = beamer.locator("#writingPromptText");
    await expect(beamerPromptText).toContainText(
      "Was siehst du auf diesem Bild?",
    );

    // ============================================
    // VERIFY: Player sees the image in writing screen
    // ============================================
    console.log("Multimodal prompt test: Verifying player displays image...");

    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    // Check that the prompt image is visible on player screen
    const playerImage = players[0].locator("#promptImage img");
    await expect(playerImage).toBeVisible({ timeout: 5000 });

    // Verify the image URL is correct
    const playerImageSrc = await playerImage.getAttribute("src");
    expect(playerImageSrc).toBe(imageUrl);

    // Verify text prompt is also shown
    const playerPromptText = players[0].locator("#promptText");
    await expect(playerPromptText).toContainText(
      "Was siehst du auf diesem Bild?",
    );

    console.log("Multimodal prompt test completed successfully!");
  });

  test("image-only prompt (no text) displays correctly", async () => {
    const { host, beamer, players } = clients;

    // Setup: Navigate to host page (game already reset by beforeEach)
    await host.goto("/host");
    await waitForConnection(host);

    // Create player
    await host.click('.sidebar-item:has-text("Spieler")');
    await host.waitForSelector("#players.active");
    await host.fill("#playerCount", "1");
    await host.click('#players button:has-text("Spieler erstellen")');
    await host.waitForTimeout(500);

    const tokens = await getPlayerTokens(host);
    expect(tokens).toHaveLength(1);

    // Register player
    await players[0].goto("/player.html");
    await players[0].fill("#tokenInput", tokens[0]);
    await players[0].click("#joinButton");
    await players[0].waitForSelector("#registerScreen.active");
    await players[0].fill("#nameInput", "ImageOnlyTester");
    await players[0].click("#registerButton");
    await players[0].waitForSelector("#waitingScreen.active");

    // Connect beamer
    await beamer.goto("/beamer.html");
    await waitForConnection(beamer);

    // ============================================
    // TEST: Add image-only prompt (no text)
    // ============================================
    console.log("Image-only prompt test: Adding prompt with image only...");

    await host.click('.sidebar-item:has-text("Prompts")');
    await host.waitForSelector("#prompts.active");

    // Clear any existing text, only fill image URL
    await host.fill("#promptText", "");

    // Expand the multimodal image details section
    await host.click('summary:has-text("Bild hinzufügen")');
    await host.waitForSelector("#promptImageUrl", { state: "visible" });
    const imageUrl = "/img/manekineko.gif";
    await host.fill("#promptImageUrl", imageUrl);

    // Add prompt to pool and queue it
    await host.click('#prompts button:has-text("Prompt hinzufügen")');
    await host.waitForSelector("#hostPromptsList .prompt-row");
    await host.locator("#hostPromptsList .prompt-row .queue-btn").first().click();

    // Wait for start button to become visible (triggered by server response)
    await host.waitForSelector("#startPromptSelectionBtn", {
      state: "visible",
      timeout: 5000,
    });

    // Start prompt selection (auto-advances to WRITING with 1 prompt)
    await host.click("#startPromptSelectionBtn");
    await host.waitForTimeout(1000);

    // ============================================
    // VERIFY: Beamer shows the image
    // ============================================
    console.log("Image-only prompt test: Verifying beamer displays image...");

    await waitForBeamerScene(beamer, "sceneWriting", 10000);

    const beamerImage = beamer.locator("#writingPromptImage img");
    await expect(beamerImage).toBeVisible({ timeout: 5000 });

    // ============================================
    // VERIFY: Player sees the image
    // ============================================
    console.log("Image-only prompt test: Verifying player displays image...");

    await players[0].waitForSelector("#writingScreen.active", {
      timeout: 5000,
    });

    const playerImage = players[0].locator("#promptImage img");
    await expect(playerImage).toBeVisible({ timeout: 5000 });

    console.log("Image-only prompt test completed successfully!");
  });
});
