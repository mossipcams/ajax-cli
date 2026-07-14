// Exploratory UI e2e: drive Web Cockpit through common operator flows via
// real clicks (not only hash navigations), and fail closed on page errors,
// console errors, or JWT-shaped leaks.

import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  terminalToolbar,
  waitForTerminalSocket,
} from "./fixtures";
import {
  installJwtLeakProbe,
  snapshotBrowserSurfaces,
  collectContinuousFindings,
  assertNoJwts,
  type JwtFinding,
} from "./jwtLeakScan";

test("exploratory UI flows stay clean of errors and JWT leaks", async ({ page }) => {
  const pageErrors: string[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(err.message));
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });

  await mockFetch(page);
  await mockTerminalWebSocket(page);
  const { consoleBuffer } = await installJwtLeakProbe(page);

  const findings: JwtFinding[] = [];
  const dashboard = page.locator("[data-testid='outlet-dashboard']");

  // 1. Dashboard — filter by project pill
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  findings.push(...(await snapshotBrowserSurfaces(page, "dashboard")));

  await page.locator("button.project-pill").filter({ hasText: "web" }).first().click();
  await expect(page.getByText("web/fix-login")).toBeVisible();
  await expect(page.getByText("api/add-auth")).not.toBeVisible();

  // 2. Open task detail via click-through, then Back proves navigation
  await page.getByText("web/fix-login").click();
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });

  // 3. Task detail — terminal, toolbar Esc, JWT snapshot
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  await terminalToolbar(page).getByRole("button", { name: "Esc" }).click();
  await page.waitForTimeout(60);
  findings.push(...(await snapshotBrowserSurfaces(page, "task-detail")));

  await page.getByRole("button", { name: "← Back" }).click();
  await expect(dashboard).toBeVisible();

  // 4. Settings via header link
  await page.locator("button.settings-link").click();
  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({
    timeout: 5_000,
  });
  findings.push(...(await snapshotBrowserSurfaces(page, "settings")));

  await page.locator(".settings-back").click();
  await expect(dashboard).toBeVisible();

  // 5. New-task sheet — open, verify input, cancel
  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  await expect(page.locator("#new-task-title-input")).toBeVisible();
  findings.push(...(await snapshotBrowserSurfaces(page, "new-task")));

  const cancel = page.getByRole("button", { name: "Cancel" });
  if (await cancel.isVisible()) {
    await cancel.click();
    await expect(page.locator("[data-testid='new-task-sheet']")).toHaveCount(0);
  }

  findings.push(
    ...(await collectContinuousFindings(page, consoleBuffer, "continuous")),
  );

  assertNoJwts(findings);
  expect(pageErrors).toEqual([]);
  expect(consoleErrors).toEqual([]);
});
