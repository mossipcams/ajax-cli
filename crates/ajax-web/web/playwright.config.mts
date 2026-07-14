import { defineConfig, devices } from "@playwright/test";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "list",
  use: {
    baseURL: "http://localhost:5173",
    trace: "on-first-retry",
  },
  projects: [
    { name: "desktop-chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "mobile-webkit", use: { ...devices["iPhone 15 Pro"] } },
  ],
  webServer: {
    command: "./node_modules/.bin/vite --config crates/ajax-web/web/vite.config.mts",
    url: "http://localhost:5173/app.html",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    cwd: repoRoot,
  },
});
