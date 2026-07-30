// Operator-flow smoke suite. API responses are mocked via addInitScript
// (overrides globalThis.fetch before the app boots) so these tests run
// without a live Rust server. They verify hash routing, dashboard
// rendering, project filtering, task detail rendering, and action
// confirmation (single-tap vs two-tap) flows in a real browser.

import { test, expect } from "@playwright/test";
import {
  mockFetch,
  rosterRow,
} from "./fixtures";

// ---- tests ---------------------------------------------------------------

// Rows and the Next card both show `qualified_handle`, not `title`, so handles
// are the stable selectors. The highest-severity inbox task renders as the Next
// card rather than a queue row.

test("dashboard renders tasks from cockpit fixture", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");

  // Inbox card shows the handle and status_explanation
  await expect(rosterRow(page, "web/fix-login")).toBeVisible({ timeout: 10_000 });
  // Calm group shows api/add-auth handle in a task row
  await expect(rosterRow(page, "api/add-auth")).toBeVisible();
});

test("project filter shows only matching repo tasks", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(rosterRow(page, "web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Pick "web" in the native repo picker (v2 replaced the pill row with a select)
  await page.getByTestId("repo-select").selectOption("web");

  await expect(rosterRow(page, "web/fix-login")).toBeVisible();
  await expect(rosterRow(page, "api/add-auth")).not.toBeVisible();
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
