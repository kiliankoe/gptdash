import { test, expect, type BrowserContext } from "@playwright/test";
import {
  type GameClients,
  waitForConnection,
  createGameClients,
  closeContexts,
  resetGameState,
} from "./test-utils";

/**
 * Model selection tests
 *
 * These tests verify the AI model selection feature works correctly.
 *
 * Basic tests (UI, API structure) run without OpenAI.
 * OpenAI integration tests require:
 *   E2E_ENABLE_OPENAI=1 OPENAI_API_KEY=sk-... npx playwright test model-selection
 */
test.describe("Model Selection", () => {
  let contexts: BrowserContext[] = [];
  let clients: GameClients;

  // Check if OpenAI is enabled for e2e tests
  // Requires both E2E_ENABLE_OPENAI=1 and OPENAI_API_KEY to be set
  const hasOpenAI =
    process.env.E2E_ENABLE_OPENAI === "1" && !!process.env.OPENAI_API_KEY;

  test.beforeEach(async ({ browser }) => {
    const result = await createGameClients(browser);
    clients = result.clients;
    contexts = result.contexts;
    await resetGameState(browser);
  });

  test.afterEach(async () => {
    await closeContexts(contexts);
  });

  test("model selector UI is visible in host panel", async () => {
    const { host } = clients;

    await host.goto("/host");
    await waitForConnection(host);

    // Navigate to the AI management section (Antworten panel)
    await host.click('.sidebar-item:has-text("Antworten")');
    await host.waitForSelector("#submissions.active");

    // The model selector should be visible
    const modelSelector = host.locator("#modelSelector");
    await expect(modelSelector).toBeVisible();

    // Should have the default option
    await expect(modelSelector.locator('option[value=""]')).toHaveText(
      "Alle Provider (Standard)",
    );
  });

  test("/api/models endpoint returns expected structure", async ({
    request,
  }) => {
    const response = await request.get("/api/models");
    expect(response.ok()).toBeTruthy();

    const data = await response.json();

    // Should have the expected structure
    expect(data).toHaveProperty("openai_models");
    expect(data).toHaveProperty("ollama_models");
    expect(data).toHaveProperty("default_model");

    // Arrays should exist (may be empty if providers not configured)
    expect(Array.isArray(data.openai_models)).toBeTruthy();
    expect(Array.isArray(data.ollama_models)).toBeTruthy();
  });

  test("model selector populates with OpenAI models when configured", async ({
    request,
  }) => {
    // This test checks the API response structure when OpenAI is configured
    // It will pass even without OpenAI key (just showing empty list)
    const response = await request.get("/api/models");
    const data = await response.json();

    if (hasOpenAI) {
      // If OpenAI is configured, should have models
      expect(data.openai_models.length).toBeGreaterThan(0);

      // Each model should have required fields
      for (const model of data.openai_models) {
        expect(model).toHaveProperty("id");
        expect(model).toHaveProperty("name");
        expect(model).toHaveProperty("supports_vision");
        expect(model.id).toMatch(/^openai:/);
      }

      // Should include expected models
      const modelIds = data.openai_models.map((m: { id: string }) => m.id);
      expect(modelIds).toContain("openai:gpt-4o-mini");
    } else {
      // Without OpenAI key, list should be empty
      expect(data.openai_models).toHaveLength(0);
    }
  });

  // Tests that require actual OpenAI API calls
  test.describe("OpenAI Integration", () => {
    test.skip(!hasOpenAI, "Skipping: OPENAI_API_KEY not set");

    test("can generate AI response with specific model (gpt-4o-mini)", async () => {
      const { host } = clients;

      await host.goto("/host");
      await waitForConnection(host);

      // Create a player
      await host.click('.sidebar-item:has-text("Spieler")');
      await host.waitForSelector("#players.active");
      await host.fill("#playerCount", "1");
      await host.click('#players button:has-text("Spieler erstellen")');
      await host.waitForSelector("#playerTokensList .token");

      // Add a prompt
      await host.click('.sidebar-item:has-text("Prompts")');
      await host.waitForSelector("#prompts.active");
      await host.fill("#promptText", "What is 2+2? Answer in one word.");
      await host.click('#prompts button:has-text("Prompt hinzufügen")');
      await host.waitForSelector("#hostPromptsList [data-prompt-id]");

      // Queue and start the round
      await host.locator("#hostPromptsList .queue-btn").first().click();
      await host.waitForSelector("#startPromptSelectionBtn", {
        state: "visible",
        timeout: 5000,
      });

      // Select gpt-4o-mini model BEFORE starting the round
      await host.click('.sidebar-item:has-text("Antworten")');
      await host.waitForSelector("#submissions.active");
      await host.selectOption("#modelSelector", "openai:gpt-4o-mini");

      // Go back and start the round
      await host.click('.sidebar-item:has-text("Prompts")');
      await host.waitForSelector("#prompts.active");
      await host.click("#startPromptSelectionBtn");

      // Wait for WRITING phase and AI generation
      await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
        timeout: 10000,
      });

      // Navigate to submissions and wait for AI submission
      await host.click('.sidebar-item:has-text("Antworten")');
      await host.waitForSelector("#submissions.active");

      // Wait for AI submission to appear (may take a few seconds for API call)
      await host.waitForSelector(".ai-submission-card", { timeout: 30000 });

      // Verify the AI submission shows the correct model
      const aiCard = host.locator(".ai-submission-card").first();
      await expect(aiCard).toBeVisible();

      // The provider badge should show "openai"
      await expect(aiCard.locator(".provider-badge")).toHaveText("openai");

      // The model name should show "gpt-4o-mini"
      await expect(aiCard.locator(".model-name")).toHaveText("gpt-4o-mini");
    });

    test("can generate AI response with different model (gpt-3.5-turbo)", async () => {
      const { host } = clients;

      await host.goto("/host");
      await waitForConnection(host);

      // Create a player
      await host.click('.sidebar-item:has-text("Spieler")');
      await host.waitForSelector("#players.active");
      await host.fill("#playerCount", "1");
      await host.click('#players button:has-text("Spieler erstellen")');
      await host.waitForSelector("#playerTokensList .token");

      // Add a prompt
      await host.click('.sidebar-item:has-text("Prompts")');
      await host.waitForSelector("#prompts.active");
      await host.fill("#promptText", "What is 3+3? Answer in one word.");
      await host.click('#prompts button:has-text("Prompt hinzufügen")');
      await host.waitForSelector("#hostPromptsList [data-prompt-id]");

      // Queue the prompt
      await host.locator("#hostPromptsList .queue-btn").first().click();
      await host.waitForSelector("#startPromptSelectionBtn", {
        state: "visible",
        timeout: 5000,
      });

      // Select gpt-3.5-turbo model BEFORE starting the round
      await host.click('.sidebar-item:has-text("Antworten")');
      await host.waitForSelector("#submissions.active");
      await host.selectOption("#modelSelector", "openai:gpt-3.5-turbo");

      // Go back and start the round
      await host.click('.sidebar-item:has-text("Prompts")');
      await host.waitForSelector("#prompts.active");
      await host.click("#startPromptSelectionBtn");

      // Wait for WRITING phase
      await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
        timeout: 10000,
      });

      // Navigate to submissions and wait for AI submission
      await host.click('.sidebar-item:has-text("Antworten")');
      await host.waitForSelector("#submissions.active");

      // Wait for AI submission to appear
      await host.waitForSelector(".ai-submission-card", { timeout: 30000 });

      // Verify the AI submission shows the correct model
      const aiCard = host.locator(".ai-submission-card").first();
      await expect(aiCard).toBeVisible();

      // The provider badge should show "openai"
      await expect(aiCard.locator(".provider-badge")).toHaveText("openai");

      // The model name should show "gpt-3.5-turbo"
      await expect(aiCard.locator(".model-name")).toHaveText("gpt-3.5-turbo");
    });

    test("regenerate AI uses selected model", async () => {
      const { host } = clients;

      await host.goto("/host");
      await waitForConnection(host);

      // Create a player
      await host.click('.sidebar-item:has-text("Spieler")');
      await host.waitForSelector("#players.active");
      await host.fill("#playerCount", "1");
      await host.click('#players button:has-text("Spieler erstellen")');
      await host.waitForSelector("#playerTokensList .token");

      // Add and queue a prompt
      await host.click('.sidebar-item:has-text("Prompts")');
      await host.waitForSelector("#prompts.active");
      await host.fill("#promptText", "What color is the sky? One word.");
      await host.click('#prompts button:has-text("Prompt hinzufügen")');
      await host.waitForSelector("#hostPromptsList [data-prompt-id]");
      await host.locator("#hostPromptsList .queue-btn").first().click();
      await host.waitForSelector("#startPromptSelectionBtn", {
        state: "visible",
        timeout: 5000,
      });

      // Start with default (all providers)
      await host.click("#startPromptSelectionBtn");
      await expect(host.locator("#overviewPhase")).toHaveText("WRITING", {
        timeout: 10000,
      });

      // Navigate to submissions
      await host.click('.sidebar-item:has-text("Antworten")');
      await host.waitForSelector("#submissions.active");

      // Wait for initial AI submission
      await host.waitForSelector(".ai-submission-card", { timeout: 30000 });

      // Count initial AI submissions
      const initialCount = await host.locator(".ai-submission-card").count();

      // Now select a specific model and regenerate
      await host.selectOption("#modelSelector", "openai:gpt-4o-mini");

      // Click regenerate button
      await host.click('button:has-text("KI neu generieren")');

      // Wait for new submission to appear
      await host.waitForTimeout(5000);

      // Should have at least one more AI submission
      const newCount = await host.locator(".ai-submission-card").count();
      expect(newCount).toBeGreaterThan(initialCount);

      // The newest submission should be from gpt-4o-mini
      // (submissions are displayed newest first or we can check any has the model)
      const gpt4oMiniCard = host.locator(
        '.ai-submission-card:has(.model-name:text("gpt-4o-mini"))',
      );
      await expect(gpt4oMiniCard).toBeVisible();
    });
  });
});
