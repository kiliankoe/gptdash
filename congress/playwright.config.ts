import { defineConfig, devices } from "@playwright/test";
import { config } from "dotenv";

// Load .env file for webServer process
config();

const webServerEnv = {
  ...process.env,
  OPENAI_API_KEY: "",
  OLLAMA_BASE_URL: "",
  OLLAMA_MODEL: "",
  // E2E tests drive the host UI without credentials.
  // Force-disable host basic auth even if it's set in the developer environment.
  HOST_USERNAME: "",
  HOST_PASSWORD: "",
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
    stdout: "pipe",
    stderr: "pipe",
    env: webServerEnv,
  },

  // Increase timeout for multi-role coordination
  timeout: 60000,
  expect: {
    timeout: 10000,
  },
});
