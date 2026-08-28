import { test, expect } from "@playwright/test";
import { mockFetch } from "./fixtures";

test("control page shows server status and lifecycle actions", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/control");
  await expect(page.getByTestId("runtime-control-status")).toBeVisible();
  await expect(page.getByTestId("runtime-restart")).toBeVisible();
  await expect(page.getByTestId("runtime-update")).toBeVisible();
});
