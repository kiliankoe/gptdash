import { defineConfig, devices } from "@playwright/test";
import { config } from "dotenv";

// Load .env file for webServer process
config();

// Allow OPENAI_API_KEY to pass through for model-selection tests when set
// Set E2E_ENABLE_OPENAI=1 to enable OpenAI for e2e tests
const enableOpenAI = process.env.E2E_ENABLE_OPENAI === "1";

// Set E2E_QUIET=1 to suppress server logs
const quietMode = process.env.E2E_QUIET === "1";

const webServerEnv = {
  ...process.env,
  BIND_ADDR: "127.0.0.1",
  // Only pass through OpenAI key if explicitly enabled for e2e tests
  OPENAI_API_KEY: enableOpenAI ? process.env.OPENAI_API_KEY || "" : "",
  OLLAMA_BASE_URL: "",
  OLLAMA_MODEL: "",
  // E2E tests drive the host UI without credentials.
  // Force-disable host basic auth even if it's set in the developer environment.
  HOST_USERNAME: "",
  HOST_PASSWORD: "",
  // E2E tests use real browsers which set navigator.webdriver=true.
  // Skip all vote anti-automation checks (webdriver + timing) for tests.
  SKIP_VOTE_ANTI_AUTOMATION: "1",
  // Disable auto-save/load to prevent e2e tests from polluting each other
  DISABLE_AUTO_SAVE: "1",
};

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false, // Tests coordinate multiple roles, run sequentially
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1, // Single worker since tests share server state
  reporter: [["html"], ["list"]],

  use: {
    baseURL: "http://localhost:6573",
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "on-first-retry",
  },

  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],

  webServer: {
    command: "cargo run",
    url: "http://localhost:6573",
    reuseExistingServer: !process.env.CI,
    timeout: 120000, // 2 minutes for cargo build + server start
    stdout: quietMode ? "ignore" : "pipe",
    stderr: quietMode ? "ignore" : "pipe",
    env: webServerEnv,
  },

  // Increase timeout for multi-role coordination
  timeout: 60000,
  expect: {
    timeout: 10000,
  },
});
