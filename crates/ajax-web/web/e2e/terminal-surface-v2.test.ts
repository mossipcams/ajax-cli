// Surface V2 (wterm) must mount on mobile WebKit without the yellow init banner.
// This is the bake-off gate CI was missing — unit/jsdom tests stayed green while
// device Safari failed. Init failure unmounts the panel and shows the banner, so
// settle on either outcome before asserting success.

import { test, expect, type Page } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalFrames,
  waitForTerminalSocket,
} from "./fixtures";

async function enableSurfaceV2(page: import("@playwright/test").Page) {
  await page.addInitScript(() => {
    window.localStorage.setItem("ajax.terminal.surfaceV2", "true");
  });
}

const EARLY_HIST_MARKER = "SURF2-HIST-000";
const LATE_HIST_MARKER = "SURF2-HIST-099";
const NEW_OUTPUT_MARKER = "SURF2-NEW-OUT";

function numberedHistoryChunk(count: number): string {
  let out = "";
  for (let i = 0; i < count; i += 1) {
    out += `SURF2-HIST-${String(i).padStart(3, "0")}\r\n`;
  }
  return out;
}

function countMarker(text: string, marker: string): number {
  let count = 0;
  let pos = 0;
  while ((pos = text.indexOf(marker, pos)) !== -1) {
    count += 1;
    pos += marker.length;
  }
  return count;
}

async function emitSurfaceV2Output(page: Page, text: string) {
  const delivered = await page.evaluate((payload) => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{ emitMessage: (d: string) => void }>;
      }
    ).__terminalSockets;
    sockets[sockets.length - 1].emitMessage(payload);
    return true;
  }, text);
  expect(delivered).toBe(true);
}

async function panelText(page: Page): Promise<string> {
  const wtermPanel = page.locator(
    '[data-testid="task-terminal-panel"][data-terminal-engine="wterm"]',
  );
  return (await wtermPanel.textContent()) ?? "";
}

async function scrollHostAwayFromBottom(page: Page) {
  await page.evaluate(() => {
    const host = document.querySelector(".wterm-host") as HTMLElement | null;
    if (!host) throw new Error("missing .wterm-host");
    host.scrollTop = 0;
    host.dispatchEvent(new Event("scroll", { bubbles: true }));
  });
}

async function hostIsAwayFromBottom(page: Page): Promise<boolean> {
  return page.evaluate(() => {
    const host = document.querySelector(".wterm-host") as HTMLElement | null;
    if (!host) return false;
    return host.scrollHeight - host.scrollTop - host.clientHeight > 5;
  });
}

async function hostIsAtBottom(page: Page): Promise<boolean> {
  return page.evaluate(() => {
    const host = document.querySelector(".wterm-host") as HTMLElement | null;
    if (!host) return false;
    return host.scrollHeight - host.scrollTop - host.clientHeight < 5;
  });
}

async function assertMarkersUnique(page: Page) {
  const text = await panelText(page);
  expect(countMarker(text, EARLY_HIST_MARKER)).toBe(1);
  expect(countMarker(text, LATE_HIST_MARKER)).toBe(1);
  expect(countMarker(text, NEW_OUTPUT_MARKER)).toBe(1);
}

async function surfaceV2FailureContext(page: import("@playwright/test").Page) {
  return page.evaluate(() => ({
    banner: document.querySelector('[data-testid="terminal-surface-v2-error"]')?.textContent ?? null,
    lastError: sessionStorage.getItem("ajax.terminal.surfaceV2.lastError"),
    engines: [...document.querySelectorAll("[data-terminal-engine]")].map((el) =>
      el.getAttribute("data-terminal-engine"),
    ),
  }));
}

test("Surface V2 mounts wterm on mobile webkit without yellow init failure", async ({
  page,
}, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "Safari/WebKit is the bake-off target for Surface V2",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await enableSurfaceV2(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  const pageErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(String(err)));
  page.on("console", (msg) => {
    if (msg.type() === "error") pageErrors.push(`console: ${msg.text()}`);
  });

  await page.goto("/app.html#/t/web%2Ffix-login");

  const errorBanner = page.getByTestId("terminal-surface-v2-error");
  const wtermPanel = page.locator(
    '[data-testid="task-terminal-panel"][data-terminal-engine="wterm"]',
  );
  const termGrid = wtermPanel.locator(".term-grid");

  // Init failure swaps the panel for the yellow banner — wait for a settled outcome.
  await Promise.race([
    termGrid.waitFor({ state: "visible", timeout: 20_000 }),
    errorBanner.waitFor({ state: "visible", timeout: 20_000 }),
  ]).catch(async () => {
    const ctx = await surfaceV2FailureContext(page);
    throw new Error(
      `Surface V2 never settled (no .term-grid, no yellow banner).\n` +
        `context=${JSON.stringify(ctx)}\npageErrors=${JSON.stringify(pageErrors)}`,
    );
  });

  if (await errorBanner.isVisible().catch(() => false)) {
    const ctx = await surfaceV2FailureContext(page);
    throw new Error(
      `Surface V2 yellow banner still showing.\n` +
        `context=${JSON.stringify(ctx)}\npageErrors=${JSON.stringify(pageErrors)}`,
    );
  }

  await expect(errorBanner).toHaveCount(0);
  await expect(wtermPanel).toBeVisible();
  await expect(termGrid).toBeVisible();
  await waitForTerminalSocket(page);

  // Host must stay cooler dark (#1e1e1e) — not warm paper brown, and not a
  // solid mustard/olive fill (the device yellow-wash bug).
  const hostBg = await page.evaluate(() => {
    const host = document.querySelector(".wterm-host");
    return host ? getComputedStyle(host).backgroundColor : null;
  });
  expect(hostBg).toMatch(/rgba?\(\s*30\s*,\s*30\s*,\s*30/);

  await page.evaluate(() => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{ emitMessage: (d: string) => void }>;
      }
    ).__terminalSockets;
    sockets[sockets.length - 1].emitMessage("Hello from Surface V2\r\n");
  });

  await expect
    .poll(async () => (await wtermPanel.textContent()) ?? "", { timeout: 10_000 })
    .toContain("Hello from Surface V2");

  // tmux paints the bottom row (status/message line) with a colored bg.
  // @wterm/dom's renderer copies the bottom-right cell bg onto .term-grid as
  // an INLINE style — the whole-terminal yellow/green wash on device. The
  // grid background must stay cooler dark (#1e1e1e) regardless.
  await page.evaluate(() => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{ emitMessage: (d: string) => void }>;
      }
    ).__terminalSockets;
    sockets[sockets.length - 1].emitMessage("\x1b[999;1H\x1b[43m\x1b[2Kstatus\x1b[0m");
  });

  // Prove the write rendered before checking the background.
  await expect
    .poll(async () => (await wtermPanel.textContent()) ?? "", { timeout: 10_000 })
    .toContain("status");

  const gridBg = await page.evaluate(() => {
    const grid = document.querySelector(".term-grid");
    return grid ? getComputedStyle(grid).backgroundColor : null;
  });
  expect(gridBg).toMatch(/rgba?\(\s*30\s*,\s*30\s*,\s*30/);

  const rowPaint = await page.evaluate(() => {
    const neutral = "rgb(30, 30, 30)";
    const rows = [...document.querySelectorAll(".term-row")];
    const rowBackgrounds = rows.map((row) => getComputedStyle(row).backgroundColor);
    const rowShadows = rows.map((row) => getComputedStyle(row).boxShadow);
    const spanBackgrounds = rows.flatMap((row) =>
      [...row.querySelectorAll("span")].map((span) => getComputedStyle(span).backgroundColor),
    );
    return {
      rowCount: rows.length,
      rowBackgrounds,
      rowShadows,
      colouredSpanCount: spanBackgrounds.filter((bg) => bg !== neutral && bg !== "rgba(0, 0, 0, 0)").length,
    };
  });
  expect(rowPaint.rowCount).toBeGreaterThan(0);
  for (const bg of rowPaint.rowBackgrounds) {
    expect(bg).toBe("rgb(30, 30, 30)");
  }
  for (const shadow of rowPaint.rowShadows) {
    expect(shadow).toBe("none");
  }
  expect(rowPaint.colouredSpanCount).toBeGreaterThan(0);
});

test("Surface V2 keeps text after a viewport resize", async ({ page }, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "iOS resizes constantly (URL bar, keyboard); WebKit is the target",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await enableSurfaceV2(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  const wtermPanel = page.locator(
    '[data-testid="task-terminal-panel"][data-terminal-engine="wterm"]',
  );
  await wtermPanel.locator(".term-grid").waitFor({ state: "visible", timeout: 20_000 });
  await waitForTerminalSocket(page);

  await page.evaluate(() => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{ emitMessage: (d: string) => void }>;
      }
    ).__terminalSockets;
    sockets[sockets.length - 1].emitMessage("resize survivor\r\n");
  });
  await expect
    .poll(async () => (await wtermPanel.textContent()) ?? "", { timeout: 10_000 })
    .toContain("resize survivor");

  // WTerm.resize() wipes the row DOM (renderer.setup) and repaints only rows
  // the core reports dirty — text must survive the rebuild.
  await page.setViewportSize({ width: 390, height: 700 });
  await expect
    .poll(async () => (await wtermPanel.textContent()) ?? "", { timeout: 10_000 })
    .toContain("resize survivor");
});

test("Surface V2 stays off Ghostty when the flag is enabled", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await enableSurfaceV2(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  const errorBanner = page.getByTestId("terminal-surface-v2-error");
  const wtermPanel = page.locator(
    '[data-testid="task-terminal-panel"][data-terminal-engine="wterm"]',
  );

  await Promise.race([
    wtermPanel.locator(".term-grid").waitFor({ state: "visible", timeout: 20_000 }),
    errorBanner.waitFor({ state: "visible", timeout: 20_000 }),
  ]);

  await expect(errorBanner).toHaveCount(0);
  await expect(wtermPanel).toBeVisible();
  await expect(page.locator('[data-terminal-engine="ghostty"]')).toHaveCount(0);
});

test("Surface V2 preserves history while scrolled up", async ({ page }, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "Scrollback hold + resize behavior is validated on mobile WebKit",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await enableSurfaceV2(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  const wtermPanel = page.locator(
    '[data-testid="task-terminal-panel"][data-terminal-engine="wterm"]',
  );
  const wtermHost = page.locator(".wterm-host");
  const newOutputButton = page.getByRole("button", { name: "New output ↓" });

  await wtermPanel.locator(".term-grid").waitFor({ state: "visible", timeout: 20_000 });
  await waitForTerminalSocket(page);

  await emitSurfaceV2Output(page, numberedHistoryChunk(100));

  await expect
    .poll(async () => wtermHost.evaluate((el) => el.classList.contains("has-scrollback")), {
      timeout: 10_000,
    })
    .toBe(true);

  await expect.poll(async () => panelText(page), { timeout: 10_000 }).toContain(EARLY_HIST_MARKER);
  await expect.poll(async () => panelText(page), { timeout: 10_000 }).toContain(LATE_HIST_MARKER);
  expect(countMarker(await panelText(page), EARLY_HIST_MARKER)).toBe(1);
  expect(countMarker(await panelText(page), LATE_HIST_MARKER)).toBe(1);

  await scrollHostAwayFromBottom(page);
  await expect.poll(() => hostIsAwayFromBottom(page), { timeout: 5_000 }).toBe(true);

  await emitSurfaceV2Output(page, `${NEW_OUTPUT_MARKER}\r\n`);

  await expect.poll(async () => panelText(page), { timeout: 10_000 }).toContain(NEW_OUTPUT_MARKER);
  await assertMarkersUnique(page);
  await expect.poll(() => hostIsAwayFromBottom(page), { timeout: 5_000 }).toBe(true);
  await expect(newOutputButton).toBeVisible({ timeout: 10_000 });

  await page.setViewportSize({ width: 390, height: 700 });
  await expect.poll(async () => panelText(page), { timeout: 10_000 }).toContain(NEW_OUTPUT_MARKER);
  await assertMarkersUnique(page);
  await expect.poll(() => hostIsAwayFromBottom(page), { timeout: 5_000 }).toBe(true);
  await expect(newOutputButton).toBeVisible({ timeout: 10_000 });

  const frameCountBefore = await page.evaluate(
    () => (window as unknown as { __terminalFrames: unknown[] }).__terminalFrames.length,
  );

  const textarea = wtermHost.locator("textarea");
  await textarea.focus();
  await page.evaluate(() => {
    const textarea = document.querySelector(".wterm-host textarea") as HTMLTextAreaElement | null;
    if (!textarea) throw new Error("missing textarea");
    textarea.focus();
    textarea.dispatchEvent(
      new KeyboardEvent("keydown", { key: "x", bubbles: true, cancelable: true }),
    );
  });

  await expect
    .poll(async () => {
      const frames = (await terminalFrames(page)) as Array<{ type?: string; data?: string }>;
      return frames.slice(frameCountBefore).some((frame) => frame.type === "input" && frame.data === "x");
    }, { timeout: 10_000 })
    .toBe(true);

  await expect.poll(() => hostIsAtBottom(page), { timeout: 10_000 }).toBe(true);
  await expect(newOutputButton).not.toBeVisible();
});

type ResizeFrame = { type: string; cols: number; rows: number };

async function resizeFrames(page: Page): Promise<ResizeFrame[]> {
  const frames = (await terminalFrames(page)) as Array<{ type?: string }>;
  return frames.filter((frame) => frame.type === "resize") as ResizeFrame[];
}

test("Surface V2 expanded viewport flexes without resizing its grid", async ({ page }, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "Expanded viewport flex behavior is validated on mobile WebKit",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await enableSurfaceV2(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  const wtermPanel = page.locator(
    '[data-testid="task-terminal-panel"][data-terminal-engine="wterm"]',
  );
  const wtermHost = page.locator(".wterm-host");

  await wtermPanel.locator(".term-grid").waitFor({ state: "visible", timeout: 20_000 });
  await waitForTerminalSocket(page);

  await emitSurfaceV2Output(page, numberedHistoryChunk(100));
  await expect.poll(async () => panelText(page), { timeout: 10_000 }).toContain(EARLY_HIST_MARKER);

  await page.getByRole("button", { name: "Expand terminal" }).click();
  await expect(wtermPanel).toHaveClass(/is-expanded/);

  const expandedHostHeight = await wtermHost.evaluate((el) => el.getBoundingClientRect().height);
  expect(expandedHostHeight).toBeGreaterThan(200);

  await page.setViewportSize({ width: 390, height: 600 });

  await expect
    .poll(async () => wtermHost.evaluate((el) => el.getBoundingClientRect().height), {
      timeout: 10_000,
    })
    .toBeLessThan(expandedHostHeight - 40);

  const shrunkHostHeight = await wtermHost.evaluate((el) => el.getBoundingClientRect().height);
  expect(shrunkHostHeight).toBeGreaterThan(200);
  expect(countMarker(await panelText(page), EARLY_HIST_MARKER)).toBe(1);

  await expect
    .poll(async () => (await resizeFrames(page)).length, { timeout: 10_000 })
    .toBeGreaterThan(0);
  const frames = await resizeFrames(page);
  for (const frame of frames) {
    expect(frame.cols).toBe(80);
    expect(frame.rows).toBe(24);
  }
});
