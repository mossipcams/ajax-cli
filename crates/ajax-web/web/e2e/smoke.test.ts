// Operator-flow smoke suite. API responses are mocked via addInitScript
// (overrides globalThis.fetch before the app boots) so these tests run
// without a live Rust server. They verify Svelte routing, rendering,
// polling, confirmation, and connection-recovery flows in a real browser.

import { test, expect } from "@playwright/test";
import {
  COCKPIT_FIXTURE,
  VERSION_A,
  VERSION_B,
  mockFetch,
} from "./fixtures";

// ---- tests ---------------------------------------------------------------

// Task list rows show `qualified_handle`, not `title`. Inbox cards also show
// `status_explanation`. Use handles as stable selectors.

test("dashboard renders tasks from cockpit fixture", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");

  // Inbox card shows the handle and status_explanation
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  // Calm group shows api/add-auth handle in a task row
  await expect(page.getByText("api/add-auth")).toBeVisible();
});

test("project filter shows only matching repo tasks", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Click the "web" project pill
  await page.locator("button.project-pill").filter({ hasText: "web" }).first().click();

  await expect(page.getByText("web/fix-login")).toBeVisible();
  await expect(page.getByText("api/add-auth")).not.toBeVisible();
});

test("task detail renders server status and actions", async ({ page }, testInfo) => {
  await mockFetch(page);
  // Use correct task hash prefix from routes.ts: #/t/
  await page.goto("/app.html#/t/web%2Ffix-login");

  if (testInfo.project.name === "mobile-webkit") {
    await expect(page.locator(".interact-pill")).toContainText("Waiting", { timeout: 10_000 });
  } else {
    await expect(page.getByText("Waiting for review")).toBeVisible({ timeout: 10_000 });
  }
  await expect(page.locator("[data-action='review']")).toBeVisible();
});

test("new task sheet stays inside the visible band when the keyboard opens", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  const titleInput = page.locator("#new-task-title-input");
  await expect(titleInput).toBeVisible();
  await titleInput.click();

  // Simulate viewport.ts reacting to the iOS soft keyboard: the visual
  // viewport shrinks to a 460px band and Safari pans the page down 40px.
  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty("--app-height", "460px");
    document.documentElement.style.setProperty("--app-top", "40px");
  });

  // The focused input must sit inside the visible band [40, 40 + 460] —
  // otherwise it is hidden behind the keyboard while the user types.
  const box = await titleInput.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.y).toBeGreaterThanOrEqual(40);
  expect(box!.y + box!.height).toBeLessThanOrEqual(40 + 460);
});

test("non-destructive action completes without a second tap", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-action='review']")).toBeVisible({ timeout: 10_000 });

  await page.locator("[data-action='review']").click();

  // Operation mock returns the refreshed cockpit; task outlet stays visible
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 5_000 });
});

test("destructive action requires two taps to execute", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-action='drop']")).toBeVisible({ timeout: 10_000 });

  // First tap: enters confirming state
  await page.locator("[data-action='drop']").click();
  await expect(page.locator(".action.confirming")).toBeVisible({ timeout: 3_000 });

  // Second tap: executes
  await page.locator(".action.confirming").click();
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 5_000 });
});

test("connection error shows backend unreachable state", async ({ page }) => {
  // Override to throw on cockpit — other routes still work
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

  await page.goto("/app.html");
  await expect(page.locator(".connection-status")).toContainText("unreachable", { timeout: 8_000 });

  // Tap Retry — cockpit is still failing, so still unreachable
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.locator(".connection-status")).toContainText("unreachable");
});

test("settings view renders restart and diagnostics controls", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");

  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByRole("button", { name: /Restart/i })).toBeVisible();
  await expect(
    page.locator("[data-testid='outlet-settings']").getByRole("button", { name: /Diagnostics/i }).first()
  ).toBeVisible();
});

test("update banner appears when version changes between polls", async ({ page }) => {
  await page.addInitScript((versions: { a: unknown; b: unknown; cockpit: unknown }) => {
    let count = 0;
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;
      if (path === "/api/cockpit")
        return new Response(
          JSON.stringify(versions.cockpit),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      if (path === "/api/version") {
        count++;
        return new Response(
          JSON.stringify(count === 1 ? versions.a : versions.b),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (path.startsWith("/api/")) {
        return new Response("{}", { status: 200, headers: { "content-type": "application/json" } });
      }
      return orig(input, init);
    };
  }, { a: VERSION_A, b: VERSION_B, cockpit: COCKPIT_FIXTURE });

  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Trigger a second version check via focus event
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));

  await expect(page.locator(".update-banner")).not.toHaveAttribute("hidden", { timeout: 10_000 });
});
