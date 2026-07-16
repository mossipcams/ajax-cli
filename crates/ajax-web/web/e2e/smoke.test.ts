// Operator-flow smoke suite. API responses are mocked via addInitScript
// (overrides globalThis.fetch before the app boots) so these tests run
// without a live Rust server. They verify Svelte routing, dashboard
// rendering, project filtering, task detail rendering, and action
// confirmation (single-tap vs two-tap) flows in a real browser.

import { test, expect } from "@playwright/test";
import { mockFetch } from "./fixtures";

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
