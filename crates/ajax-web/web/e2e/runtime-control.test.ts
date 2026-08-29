import { test, expect } from "@playwright/test";
import { mockFetch } from "./fixtures";

test("settings page shows server status and lifecycle actions", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");
  await expect(page.getByTestId("runtime-control-status")).toBeVisible();
  await expect(page.getByTestId("runtime-restart")).toBeVisible();
  await expect(page.getByTestId("runtime-update")).toBeVisible();
});

test("legacy control hash lands on settings runtime controls", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/control");
  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible();
  await expect(page.getByTestId("runtime-control-status")).toBeVisible();
  await expect(page).toHaveURL(/#\/settings$/);
});
