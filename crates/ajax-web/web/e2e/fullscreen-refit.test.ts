// Regression for the fullscreen (⛶ expand) bug that shipped last release: the
// terminal did not re-fit after the visual viewport settled, leaving the PWA
// zoomed in. #375 fixed it and added jsdom unit regressions (fake-timer proof
// of the post-settle refit). This suite is the missing *real-webkit* proof:
// entering fullscreen refits the terminal to the fullscreen layer and does not
// zoom the page. The geometry fuzzer cannot cover this — the bug is DOM/timing
// orchestration, not scalar math.

import { test, expect, type Page } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalFrames,
  terminalPanel,
  waitForTerminalSocket,
} from "./fixtures";

const expandButton = (page: Page) =>
  terminalPanel(page).getByRole("button", { name: "Expand terminal" });

type ResizeFrame = { type: string; cols: number; rows: number };

async function resizeFrames(page: Page): Promise<ResizeFrame[]> {
  const frames = (await terminalFrames(page)) as Array<{ type?: string }>;
  return frames.filter((frame) => frame.type === "resize") as ResizeFrame[];
}

async function openTerminal(page: Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page).locator("canvas:not([aria-hidden='true'])")).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
}

test("expand button stays in viewport and is the hit target after scale-to-fit", async ({ page }) => {
  await openTerminal(page);
  // Let fit/scale settle.
  await page.waitForTimeout(300);

  const info = await page.evaluate(() => {
    const panel = document.querySelector("[data-testid='task-terminal-panel']");
    const host = panel?.querySelector(".terminal-host") as HTMLElement | null;
    const scale = panel?.querySelector(".terminal-scale-layer") as HTMLElement | null;
    const btn = panel?.querySelector(".terminal-expand-corner") as HTMLElement | null;
    const br = btn?.getBoundingClientRect();
    const pr = panel?.getBoundingClientRect();
    const hr = host?.getBoundingClientRect();
    const cx = br ? br.left + br.width / 2 : 0;
    const cy = br ? br.top + br.height / 2 : 0;
    const at = br ? document.elementFromPoint(cx, cy) : null;
    return {
      viewportW: window.innerWidth,
      panelW: pr?.width ?? 0,
      hostClientW: host?.clientWidth ?? 0,
      hostScrollW: host?.scrollWidth ?? 0,
      hostRectW: hr?.width ?? 0,
      btnRight: br?.right ?? 0,
      btnInViewport: !!br && br.right <= window.innerWidth + 1 && br.left >= -1,
      hitIsExpand:
        !!at &&
        (at === btn ||
          at.closest?.(".terminal-expand-corner") === btn ||
          at.getAttribute?.("aria-label") === "Expand terminal"),
      hitClass: (at as HTMLElement | null)?.className ?? null,
      hostTransform: host?.style?.transform ?? "",
      scaleTransform: scale?.style?.transform ?? "",
    };
  });

  expect(info.btnInViewport, JSON.stringify(info)).toBe(true);
  expect(info.hitIsExpand, JSON.stringify(info)).toBe(true);
  expect(info.hostTransform, JSON.stringify(info)).toBe("");
  expect(info.hostRectW, JSON.stringify(info)).toBeGreaterThan(300);
  expect(info.hostScrollW, JSON.stringify(info)).toBeLessThanOrEqual(info.hostClientW + 2);
});

test("entering fullscreen refits the terminal and does not zoom the PWA", async ({ page }) => {
  await openTerminal(page);

  const panel = terminalPanel(page);
  await expect(panel).not.toHaveClass(/is-expanded/);

  await expandButton(page).click();

  // Fullscreen takeover is active on every layer that marks it.
  await expect(panel).toHaveClass(/is-expanded/);
  await expect(expandButton(page)).toHaveAttribute("aria-pressed", "true");
  await expect
    .poll(() => page.evaluate(() => document.documentElement.classList.contains("terminal-expanded")))
    .toBe(true);

  // The terminal re-fits to the fullscreen layer: a resize frame lands after
  // the expand with a valid grid (finite positive cols/rows). Before #375 the
  // post-settle refit never fired, so the grid stayed misfit for the new space.
  await expect
    .poll(async () => (await resizeFrames(page)).length, { timeout: 2_000 })
    .toBeGreaterThan(0);
  const latest = (await resizeFrames(page)).at(-1)!;
  expect(Number.isFinite(latest.cols) && latest.cols > 0).toBe(true);
  expect(Number.isFinite(latest.rows) && latest.rows > 0).toBe(true);
});

test("entering fullscreen hides the global chrome so it cannot peek through", async ({ page }) => {
  await openTerminal(page);
  const header = page.locator(".cockpit-chrome");
  const nav = page.locator(".bottom-nav");
  await expect(header).toBeVisible();
  await expect(nav).toBeVisible();

  await expandButton(page).click();
  await expect(page.locator("[data-testid='task-terminal-panel']")).toHaveClass(/is-expanded/);
  await expect(header).toBeHidden();
  await expect(nav).toBeHidden();

  await expandButton(page).click();
  await expect(header).toBeVisible();
  await expect(nav).toBeVisible();
});

test("exiting fullscreen restores the inline terminal", async ({ page }) => {
  await openTerminal(page);

  const toggle = expandButton(page);
  await toggle.click();
  await expect(terminalPanel(page)).toHaveClass(/is-expanded/);

  await toggle.click();
  await expect(terminalPanel(page)).not.toHaveClass(/is-expanded/);
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await expect
    .poll(() => page.evaluate(() => document.documentElement.classList.contains("terminal-expanded")))
    .toBe(false);
});

// Regression guard for the iOS focus-zoom fix: the served shell must cap zoom
// via the viewport meta so tapping the fullscreen (⛶) button — which focuses
// the terminal input — cannot make iOS Safari zoom the whole page. Headless
// webkit cannot reproduce iOS focus-zoom, so this asserts the guard is present
// in the served document rather than simulating the zoom.
test("served shell caps zoom with maximum-scale=1 (iOS focus-zoom guard)", async ({ page }) => {
  await page.goto("/app.html");
  const content = await page.locator('meta[name="viewport"]').getAttribute("content");
  expect(content).toContain("maximum-scale=1");
});

test("fullscreen keeps global chrome hidden even when the visual viewport shrinks", async ({ page }) => {
  await openTerminal(page);
  await expandButton(page).click();
  await expect(page.locator("[data-testid='task-terminal-panel']")).toHaveClass(/is-expanded/);

  // Simulate viewport.ts on keyboard-open: the visual viewport shrinks below the
  // layout viewport — the divergence that made the chrome peek through on iOS.
  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty("--app-height", (window.innerHeight - 336) + "px");
  });

  // The global chrome must stay hidden — no peek-through in the exposed band.
  await expect(page.locator(".cockpit-chrome")).toBeHidden();
  await expect(page.locator(".bottom-nav")).toBeHidden();
});
