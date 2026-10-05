// Permanent iOS-WebKit terminal behavior suite. The first test pins the
// engine-neutral application-surface locator and a single task-terminal
// WebSocket opening on the task route, without asserting on engine-specific
// DOM or renderer internals.


import { test, expect } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalSurface,
  terminalInteractionSurface,
  terminalInputFrames,
  terminalSocketSummaries,
  openLatestTerminalSocket,
  closeLatestTerminalSocket,
  failLatestTerminalSocket,
  emitLatestTerminalOutput,
  waitForTerminalSocket
} from "./fixtures";
import {
  PTY_OUTPUT_CORPUS_CHUNKS,
  activeTaskSocketCount,
  clickTerminalSurfaceInterior,
  gotoTaskRoute,
  inputFrameCount,
  newOutputButton,
  openTaskTerminal,
  scrollInteractionSurfaceAway,
  scrollbackChunk
} from "./terminal-behavior-helpers";


// Playwright requires object-destructured fixtures; empty pattern is intentional.
// eslint-disable-next-line no-empty-pattern -- Playwright beforeEach fixture contract
test.beforeEach(({}, testInfo) => {
  const desktopOnly =
    testInfo.title ===
    "desktop expanded mode keeps terminal bounded and task details summary reachable";
  if (desktopOnly) {
    test.skip(
      testInfo.project.name !== "desktop-chromium",
      "desktop expanded layout only",
    );
  } else {
    test.skip(
      testInfo.project.name !== "mobile-webkit",
      "terminal acceptance is mobile-webkit only",
    );
  }
});


test("task route mounts one terminal surface and opens one socket", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await expect(surface).toHaveCount(1);

  await waitForTerminalSocket(page);

  const sockets = await terminalSocketSummaries(page);
  expect(sockets).toHaveLength(1);
});


test("delayed socket open shows Connecting then connects", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });

  const status = page.getByTestId("terminal-status");
  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(page.getByRole("button", { name: "Reconnect" })).not.toBeVisible();

  await openLatestTerminalSocket(page);

  await expect(status).toHaveAttribute("aria-hidden", "true");
  await expect(page.getByRole("button", { name: "Reconnect" })).not.toBeVisible();
});


test("socket close reconnects, server error becomes unavailable, and manual reconnect recovers", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await openLatestTerminalSocket(page);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);

  const status = page.getByTestId("terminal-status");
  const reconnect = page.getByRole("button", { name: "Reconnect" });

  await closeLatestTerminalSocket(page);

  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(reconnect).toBeVisible();

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(2);

  await openLatestTerminalSocket(page);
  await failLatestTerminalSocket(page, "tmux session missing");

  await expect(status).toHaveAttribute("aria-hidden", "false");
  await expect(reconnect).toBeVisible();

  await reconnect.click();

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(3);

  await openLatestTerminalSocket(page);

  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});


test("navigation away closes the active socket and removes the surface", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.evaluate(() => {
    window.location.hash = "#/";
  });
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await expect(surface).not.toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(0);
});


test("pty output corpus keeps surface connected without application errors", async ({ page }) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);

  await expect(surface).toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
  expect(pageErrors).toEqual([]);
});


test("reopening the task route yields one surface and one active socket", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await page.evaluate(() => {
    window.location.hash = "#/";
  });
  await expect(surface).not.toBeVisible();

  await gotoTaskRoute(page);

  await expect(surface).toBeVisible({ timeout: 10_000 });
  await expect(surface).toHaveCount(1);
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
});


test("typing after manual reconnect sends exactly one input frame", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  await failLatestTerminalSocket(page, "tmux session missing");

  const reconnect = page.getByRole("button", { name: "Reconnect" });
  await expect(reconnect).toBeVisible();

  await reconnect.click();
  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(2);
  await openLatestTerminalSocket(page);
  await waitForTerminalSocket(page);

  const baseline = await inputFrameCount(page);

  await clickTerminalSurfaceInterior(page);
  await page.keyboard.type("!");

  await expect.poll(async () => (await inputFrameCount(page)) - baseline).toBe(1);
  const frames = await terminalInputFrames(page);
  expect(frames.at(-1)?.data).toBe("!");
});


test("seeded reconnect restores live follow at the interaction surface bottom", async ({
  page,
}) => {
  await openTaskTerminal(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  await expect(newOutputButton(page)).not.toBeVisible();

  await scrollInteractionSurfaceAway(page);

  await failLatestTerminalSocket(page, "tmux session missing");

  const reconnect = page.getByRole("button", { name: "Reconnect" });
  await expect(reconnect).toBeVisible();
  await reconnect.click();

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(2);
  await openLatestTerminalSocket(page);
  await waitForTerminalSocket(page);

  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 50), "seeded live tail\r\n"]);

  await expect(newOutputButton(page)).not.toBeVisible();
  await expect
    .poll(async () =>
      terminalInteractionSurface(page).evaluate(
        (el) =>
          el.scrollHeight <= el.clientHeight + 1 ||
          el.scrollTop + el.clientHeight >= el.scrollHeight - 1,
      ),
    )
    .toBe(true);
});


test("pty output corpus during delayed socket open keeps surface stable without application errors", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });

  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page, { autoOpen: false });

  await gotoTaskRoute(page);

  const surface = terminalSurface(page);
  await expect(surface).toBeVisible({ timeout: 10_000 });

  await expect.poll(async () => (await terminalSocketSummaries(page)).length).toBe(1);
  const socketsBeforeOpen = await terminalSocketSummaries(page);
  expect(socketsBeforeOpen[0]?.readyState).toBe(0);

  await emitLatestTerminalOutput(page, PTY_OUTPUT_CORPUS_CHUNKS);

  await openLatestTerminalSocket(page);

  const status = page.getByTestId("terminal-status");
  await expect(status).toHaveAttribute("aria-hidden", "true");
  await expect(surface).toBeVisible();
  await expect.poll(async () => activeTaskSocketCount(page)).toBe(1);
  expect(pageErrors).toEqual([]);
});


test("seeded open stays hidden until output after the seed settles, then lands at the bottom", async ({
  page,
}) => {
  await openTaskTerminal(page);
  const surface = terminalInteractionSurface(page);
  await expect(surface).toHaveClass(/is-seed-pending/);

  // The seed is scrollback only; tmux's attach repaint of the visible pane
  // arrives in a later frame. Revealing between the two is what made the
  // terminal visibly scroll a screenful on load.
  await emitLatestTerminalOutput(page, [scrollbackChunk(0, 200)]);
  // Stay under SEED_REVEAL_QUIET_MS (120) so the mid-gap assert still sees pending.
  await page.waitForTimeout(80);
  await expect(surface).toHaveClass(/is-seed-pending/);

  await emitLatestTerminalOutput(page, [scrollbackChunk(200, 40)]);
  await expect(surface).not.toHaveClass(/is-seed-pending/);
  await expect
    .poll(async () =>
      surface.evaluate(
        (el) =>
          el.scrollHeight <= el.clientHeight + 1 ||
          el.scrollTop + el.clientHeight >= el.scrollHeight - 1,
      ),
    )
    .toBe(true);
});
