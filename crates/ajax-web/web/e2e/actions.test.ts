// Button/action coverage suite: one test per interactive control not already
// exercised by smoke.test.ts. Drives the real app (webkit + chromium) through
// each button and asserts its observable effect.

import { test, expect, type Page } from "@playwright/test";
import {
  COCKPIT_FIXTURE,
  mockFetch,
} from "./fixtures";

// Record clipboard writes so Copy buttons can be asserted.
async function installClipboardSpy(page: Page) {
  await page.addInitScript(() => {
    const writes: string[] = [];
    Object.defineProperty(window, "__clipboardWrites", { value: writes, configurable: true });
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (text: string) => {
          writes.push(text);
        },
        readText: async () => "echo pasted",
      },
    });
  });
}
const clipboardWrites = (page: Page) =>
  page.evaluate(() => (window as unknown as { __clipboardWrites: string[] }).__clipboardWrites);

// Force the cockpit poll to fail so ConnectionStatus renders its action row.
async function failCockpit(page: Page) {
  await page.addInitScript(() => {
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      if (url.includes("/api/cockpit")) throw new TypeError("Failed to fetch");
      return orig(input, init);
    };
  });
}

const dashboard = (page: Page) => page.locator("[data-testid='outlet-dashboard']");
const settings = (page: Page) => page.locator("[data-testid='outlet-settings']");
const resultPanel = (page: Page) => page.locator(".result-panel");

// ---- App chrome navigation ------------------------------------------------

test("header Settings link opens the settings route", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator("button.settings-link").click();
  await expect(settings(page)).toBeVisible();
});

test("bottom-nav Dashboard returns to the dashboard route", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");
  await expect(settings(page)).toBeVisible({ timeout: 10_000 });

  await page.locator(".bottom-nav [data-bottom-route='#/']").click();
  await expect(dashboard(page)).toBeVisible();
});

// ---- TaskDetail -----------------------------------------------------------

test("task detail Back returns to the dashboard", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });

  await page.getByRole("button", { name: "← Back" }).click();
  await expect(dashboard(page)).toBeVisible();
});

test("task detail Copy buttons copy branch and worktree path", async ({ page }) => {
  // Copy buttons live in meta-details, which is desktop-only in the layout.
  await page.setViewportSize({ width: 1280, height: 800 });
  await installClipboardSpy(page);
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });

  await page.locator(".meta-details summary").click();
  const copyButtons = page.locator(".meta-copy");
  await copyButtons.nth(0).click();
  await expect.poll(() => clipboardWrites(page)).toContain("ajax/fix-login");

  await copyButtons.nth(1).click();
  await expect.poll(() => clipboardWrites(page)).toContain("/repo/web/ajax-fix-login");
});

// ---- SettingsView ---------------------------------------------------------

test("settings Back returns to the dashboard", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");
  await expect(settings(page)).toBeVisible({ timeout: 10_000 });

  await page.locator(".settings-back").click();
  await expect(dashboard(page)).toBeVisible();
});

test("settings Restart confirms then restarts the server", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");
  const restart = page.getByRole("button", { name: /Restart server/i });
  await expect(restart).toBeVisible({ timeout: 10_000 });

  // First tap arms the confirm; second tap executes.
  await restart.click();
  await expect(page.getByRole("button", { name: /Tap to confirm/i })).toBeVisible();
  await page.getByRole("button", { name: /Tap to confirm/i }).click();

  await expect(resultPanel(page)).toContainText("Server restarted", { timeout: 10_000 });
});

test("settings Run diagnostics renders a diagnostics report", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");
  await expect(settings(page)).toBeVisible({ timeout: 10_000 });

  await page.getByRole("button", { name: "Run diagnostics" }).click();
  await expect(page.locator(".settings-view pre.settings-status")).toBeVisible({ timeout: 10_000 });
});

test("settings Copy Diagnostics surfaces a result and Dismiss clears it", async ({ page }) => {
  await installClipboardSpy(page);
  await mockFetch(page);
  await page.goto("/app.html#/settings");
  await expect(settings(page)).toBeVisible({ timeout: 10_000 });

  await page.locator(".settings-view").getByRole("button", { name: /Copy Diagnostics/i }).click();
  await expect(resultPanel(page)).toContainText("Diagnostics", { timeout: 10_000 });

  await resultPanel(page).getByRole("button", { name: "Dismiss" }).click();
  await expect(resultPanel(page)).toHaveCount(0);
});

// ---- NewTaskSheet ---------------------------------------------------------

test("new task sheet Cancel closes the sheet", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  await expect(page.locator("[data-testid='new-task-sheet']")).toBeVisible();

  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.locator("[data-testid='new-task-sheet']")).toHaveCount(0);
});

test("new task sheet Start submits and reports the task started", async ({ page }) => {
  await mockFetch(page, {
    "/api/tasks": { ok: true, state_changed: true, cockpit: COCKPIT_FIXTURE, output: "started", error: null },
  });
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  const sheet = page.locator("[data-testid='new-task-sheet']");
  await expect(sheet).toBeVisible();
  await sheet.locator("#new-task-title-input").fill("Add logout");
  await sheet.getByRole("button", { name: "Start" }).click();

  await expect(page.locator("[data-testid='new-task-sheet']")).toHaveCount(0, { timeout: 10_000 });
  await expect(resultPanel(page)).toContainText("Task started");
});

// Keyboard traversal of the agent picker, driven the way a user reaches it: Tab in
// from the title field, then arrow. jsdom cannot cover this — it does not implement
// the focus semantics — and a Radix RadioGroup silently failed exactly here, leaving
// the unselected agents unreachable. This is the test that caught it.
test("agent picker is keyboard reachable and moves with arrow keys", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  const sheet = page.locator("[data-testid='new-task-sheet']");
  await expect(sheet).toBeVisible();

  await sheet.locator("#new-task-title-input").focus();
  await page.keyboard.press("Tab");
  await expect(sheet.getByRole("radio", { name: "Codex" })).toBeFocused();

  await page.keyboard.press("ArrowRight");
  await expect(sheet.getByRole("radio", { name: "Claude" })).toBeFocused();
  await expect(sheet.getByRole("radio", { name: "Claude" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(sheet.getByRole("radio", { name: "Codex" })).toHaveAttribute(
    "aria-checked",
    "false",
  );
});

// ---- ConnectionStatus (error-state action row) ----------------------------

test("connection Copy Diagnostics jumps to the settings route", async ({ page }) => {
  await failCockpit(page);
  await page.goto("/app.html");
  await expect(page.locator(".connection-status")).toContainText("unreachable", { timeout: 10_000 });

  await page.locator(".connection-actions").getByRole("button", { name: /Copy Diagnostics/i }).click();
  await expect(settings(page)).toBeVisible();
});

test("connection Reload calls location.reload", async ({ page }) => {
  await failCockpit(page);
  await page.goto("/app.html");
  await expect(page.locator(".connection-status")).toContainText("unreachable", { timeout: 10_000 });

  const reload = page.locator(".connection-actions").getByRole("button", { name: "Reload" });
  await expect(reload).toBeVisible();
  await expect(reload).toBeEnabled();
  await Promise.all([page.waitForEvent("load"), reload.click()]);
});

test("connection Retry recovers when cockpit becomes reachable", async ({ page }) => {
  await mockFetch(page);
  await failCockpit(page);
  await page.goto("/app.html");
  const banner = page.locator(".connection-status");
  await expect(banner).toContainText("unreachable", { timeout: 10_000 });

  await page.evaluate((fixture) => {
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      if (url.includes("/api/cockpit")) {
        return new Response(JSON.stringify(fixture), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return orig(input, init);
    };
  }, COCKPIT_FIXTURE);

  await banner.getByRole("button", { name: "Retry" }).click();
  await expect(banner).toHaveAttribute("data-state", "connected", { timeout: 10_000 });
  await expect(banner).not.toContainText("unreachable");
});
