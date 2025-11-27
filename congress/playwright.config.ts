import { defineConfig, devices } from "@playwright/test";
import { config } from "dotenv";

// Load .env file for webServer process
config();

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
  },

  // Increase timeout for multi-role coordination
  timeout: 60000,
  expect: {
    timeout: 10000,
  },
});
