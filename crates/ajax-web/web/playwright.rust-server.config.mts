import { defineConfig, devices } from "@playwright/test";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

export default defineConfig({
  testDir: "./e2e",
  testMatch: "rust-server-assets.test.ts",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: "list",
  use: {
    baseURL: "https://127.0.0.1:18789",
    ignoreHTTPSErrors: true,
    trace: "on-first-retry",
  },
  projects: [{ name: "mobile-webkit", use: { ...devices["iPhone 15 Pro"] } }],
  webServer: {
    command:
      "cargo run --release -p ajax-cli -- --config target/web-smoke/config.toml --state target/web-smoke/ajax.db --worktree-root target/web-smoke/worktrees web --host 127.0.0.1 --port 18789",
    url: "https://127.0.0.1:18789/api/health",
    reuseExistingServer: false,
    timeout: 600_000,
    cwd: repoRoot,
    ignoreHTTPSErrors: true,
  },
});
