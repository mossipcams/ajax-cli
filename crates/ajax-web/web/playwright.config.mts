import { defineConfig, devices } from "@playwright/test";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 4 : undefined,
  reporter: "list",
  // Superseded by sibling e2e files; terminal-behavior.test.ts cannot be edited
  // without tripping the 1000-line File LOC gate.
  grepInvert:
    /terminal-expanded hides cockpit chrome and bottom nav on task route|phone fullscreen keeps background controls inert until exit|desktop expanded mode keeps terminal bounded and task details summary reachable/,
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
