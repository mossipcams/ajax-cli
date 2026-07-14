// HARSH terminal visual / layout hunts (mobile-webkit).
// Fail closed on blank bands, dead spacing, chrome covering PTY, keyboard
// detach, geometry jump, horizontal overflow, and scale fill failures.

import { test, expect, type Page } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  waitForTerminalSocket,
} from "./fixtures";

test.beforeEach(async ({}, testInfo) => {
  test.skip(testInfo.project.name !== "mobile-webkit", "harsh terminal visual: WebKit only");
});

const canvas = (page: Page) =>
  terminalPanel(page).locator("canvas:not([aria-hidden='true'])");

async function openTerminal(page: Page, size = { width: 390, height: 844 }) {
  await page.setViewportSize(size);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await expect(canvas(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
  await page.waitForTimeout(400);
}

async function setKeyboard(page: Page, open: boolean, top = 48, height = 420) {
  await page.evaluate(
    ({ open: isOpen, top: t, height: h }) => {
      const root = document.documentElement;
      if (isOpen) {
        root.classList.add("keyboard-open");
        root.style.setProperty("--app-top", `${t}px`);
        root.style.setProperty("--app-height", `${h}px`);
      } else {
        root.classList.remove("keyboard-open");
        root.style.removeProperty("--app-top");
        root.style.removeProperty("--app-height");
      }
      window.dispatchEvent(new Event("resize"));
    },
    { open, top, height },
  );
  await page.waitForTimeout(200);
}

async function emitOutput(page: Page, text: string) {
  const ok = await page.evaluate((payload) => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{ url?: string; emitMessage(d: string): void }>;
      }
    ).__terminalSockets;
    const socket = [...sockets]
      .reverse()
      .find((s) => typeof s.url === "string" && s.url.includes("/terminal"));
    if (!socket) return false;
    socket.emitMessage(payload);
    return true;
  }, text);
  expect(ok).toBe(true);
}

/** Full geometry dump used by every harsh assertion. */
async function geo(page: Page) {
  return page.evaluate(() => {
    const panel = document.querySelector(
      "[data-testid='task-terminal-panel']",
    ) as HTMLElement;
    const host = panel.querySelector(".terminal-host") as HTMLElement;
    const scale = panel.querySelector(".terminal-scale-layer") as HTMLElement | null;
    const canvasEl = panel.querySelector(
      "canvas:not([aria-hidden='true'])",
    ) as HTMLElement;
    const keys = panel.querySelector(".terminal-keys") as HTMLElement | null;
    const status = panel.querySelector("[data-testid='terminal-status']") as HTMLElement | null;
    const controls = panel.querySelector(
      "[data-testid='terminal-bottom-controls']",
    ) as HTMLElement | null;
    const expand = panel.querySelector(".terminal-expand-corner") as HTMLElement | null;
    const newOut = panel.querySelector(".terminal-new-output") as HTMLElement | null;
    const pr = panel.getBoundingClientRect();
    const hr = host.getBoundingClientRect();
    const cr = canvasEl.getBoundingClientRect();
    const kr = keys?.getBoundingClientRect() ?? null;
    const sr = status?.getBoundingClientRect() ?? null;
    const ctr = controls?.getBoundingClientRect() ?? null;
    const er = expand?.getBoundingClientRect() ?? null;
    const nr = newOut?.getBoundingClientRect() ?? null;
    const transform = scale?.style.transform || canvasEl.style.transform || "";
    const scaleMatch = /scale\(([^)]+)\)/.exec(transform);
    const fitScale = scaleMatch ? Number(scaleMatch[1]) : 1;
    const appTop = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-top") || "0",
    ) || 0;
    const appHeight =
      Number.parseFloat(
        getComputedStyle(document.documentElement).getPropertyValue("--app-height") || "0",
      ) || window.innerHeight;
    const sampleCover = (x: number, y: number) => {
      const el = document.elementFromPoint(x, y) as HTMLElement | null;
      if (!el) return "null";
      for (const cls of [
        "detail-header",
        "interact-panel",
        "meta-details",
        "cockpit-chrome",
        "bottom-nav",
        "terminal-status",
        "terminal-expand-corner",
        "terminal-new-output",
        "terminal-copy-overlay",
      ]) {
        if (el.closest(`.${cls}`)) return cls;
      }
      if (el.closest("canvas")) return "canvas";
      if (el.closest(".terminal-host")) return "host";
      if (el.closest(".terminal-keys")) return "keys";
      return `${el.tagName}.${String(el.className).slice(0, 40)}`;
    };
    const cx = cr.left + cr.width / 2;
    const cy = cr.top + cr.height / 2;
    return {
      viewport: { w: window.innerWidth, h: window.innerHeight },
      band: { top: appTop, bottom: appTop + appHeight, height: appHeight },
      panel: { t: pr.top, b: pr.bottom, l: pr.left, r: pr.right, h: pr.height, w: pr.width },
      host: {
        t: hr.top,
        b: hr.bottom,
        l: hr.left,
        h: hr.height,
        w: hr.width,
        clientW: host.clientWidth,
        scrollW: host.scrollWidth,
        clientH: host.clientHeight,
        scrollH: host.scrollHeight,
      },
      canvas: { t: cr.top, b: cr.bottom, l: cr.left, h: cr.height, w: cr.width },
      keys: kr ? { t: kr.top, b: kr.bottom, h: kr.height } : null,
      status: status
        ? {
            h: sr?.height ?? 0,
            empty: status.classList.contains("is-empty"),
            display: getComputedStyle(status).display,
            visibility: getComputedStyle(status).visibility,
          }
        : null,
      controls: ctr ? { t: ctr.top, b: ctr.bottom, h: ctr.height } : null,
      expand: er ? { t: er.top, b: er.bottom, r: er.right, l: er.left } : null,
      newOut: nr ? { t: nr.top, b: nr.bottom, r: nr.right } : null,
      fitScale,
      blankBelowCanvas: Math.max(0, hr.bottom - cr.bottom),
      blankRightOfCanvas: Math.max(0, hr.right - cr.right),
      hostToKeys: kr ? kr.top - hr.bottom : null,
      controlsOverlapHost: ctr ? Math.max(0, hr.bottom - ctr.top) : 0,
      coverCenter: sampleCover(cx, cy),
      coverTop: sampleCover(cx, cr.top + 6),
      coverBottom: sampleCover(cx, cr.bottom - 6),
      visualFillX: cr.width / Math.max(1, hr.width),
      visualFillY: cr.height / Math.max(1, hr.height),
    };
  });
}

// ---------------------------------------------------------------------------
// Portrait baseline — harsh
// ---------------------------------------------------------------------------

test("HARSH: scaled canvas must fill the host (no blank band / side gutter)", async ({ page }) => {
  await openTerminal(page);
  const g = await geo(page);
  expect(g.blankBelowCanvas, JSON.stringify(g)).toBeLessThanOrEqual(8);
  expect(g.blankRightOfCanvas, JSON.stringify(g)).toBeLessThanOrEqual(8);
  // If scale < 1 without row compensation, visualFillY collapses toward fitScale.
  if (g.fitScale < 0.98) {
    expect(g.visualFillY, `scale=${g.fitScale} but canvas only fills ${g.visualFillY} of host`).toBeGreaterThan(
      0.88,
    );
  }
  expect(g.host.scrollW - g.host.clientW, "horizontal glitch scroll").toBeLessThanOrEqual(2);
  expect(g.panel.r, "panel bleeds past viewport").toBeLessThanOrEqual(g.viewport.w + 1);
});

test("HARSH: empty status must not invent a dead spacer above keys", async ({ page }) => {
  await openTerminal(page);
  const g = await geo(page);
  expect(g.status?.empty, "expected connected/empty status for this probe").toBe(true);
  expect(g.hostToKeys, JSON.stringify(g)).not.toBeNull();
  expect(
    g.hostToKeys!,
    `dead spacer host→keys=${g.hostToKeys} status=${JSON.stringify(g.status)}`,
  ).toBeLessThanOrEqual(10);
  // visibility:hidden still occupying space is the defect class
  if (g.status?.visibility === "hidden") {
    expect(g.status.h, "hidden status still has layout height").toBeLessThanOrEqual(1);
  }
});

test("HARSH: chrome/meta/expand must not cover the PTY content area", async ({ page }) => {
  await openTerminal(page);
  const g = await geo(page);
  for (const [label, hit] of [
    ["center", g.coverCenter],
    ["top", g.coverTop],
    ["bottom", g.coverBottom],
  ] as const) {
    expect(
      ["canvas", "host"].includes(hit) || hit.startsWith("CANVAS"),
      `PTY ${label} covered by ${hit}`,
    ).toBe(true);
  }
  // Expand corner may sit on the panel edge but must stay inside the viewport.
  if (g.expand) {
    expect(g.expand.r).toBeLessThanOrEqual(g.viewport.w + 1);
    expect(g.expand.l).toBeGreaterThanOrEqual(-1);
  }
});

// ---------------------------------------------------------------------------
// Keyboard attachment / jump
// ---------------------------------------------------------------------------

test("HARSH: keyboard-open attaches keys to band bottom and clears chrome cover", async ({
  page,
}) => {
  await openTerminal(page);
  const before = await geo(page);
  await setKeyboard(page, true, 48, 420);
  const open = await geo(page);

  expect(open.coverCenter === "canvas" || open.coverCenter === "host", `covered by ${open.coverCenter}`).toBe(
    true,
  );
  expect(open.keys, "keys missing under keyboard").not.toBeNull();
  const detach = open.band.bottom - open.keys!.b;
  expect(detach, `keys floating ${detach}px above keyboard band`).toBeLessThanOrEqual(16);
  expect(detach, `keys below keyboard band by ${detach}`).toBeGreaterThanOrEqual(-4);
  expect(open.panel.h, `terminal collapsed under keyboard to ${open.panel.h}`).toBeGreaterThan(220);
  expect(open.hostToKeys ?? 99, `spacer under keyboard ${open.hostToKeys}`).toBeLessThanOrEqual(12);

  await setKeyboard(page, false);
  const after = await geo(page);
  expect(Math.abs(after.panel.h - before.panel.h), `jump after keyboard ${before.panel.h}->${after.panel.h}`).toBeLessThanOrEqual(
    24,
  );
  expect(Math.abs(after.canvas.h - before.canvas.h), `canvas jump ${before.canvas.h}->${after.canvas.h}`).toBeLessThanOrEqual(
    32,
  );
  expect(Math.abs(after.panel.t - before.panel.t), `panel top jump ${before.panel.t}->${after.panel.t}`).toBeLessThanOrEqual(
    24,
  );
});

test("HARSH: rapid keyboard thrash must not leave geometry wrecked", async ({ page }) => {
  await openTerminal(page);
  const baseline = await geo(page);
  for (let i = 0; i < 5; i++) {
    await setKeyboard(page, true, 30 + i * 10, 400 - i * 20);
    await setKeyboard(page, false);
  }
  const after = await geo(page);
  expect(Math.abs(after.panel.h - baseline.panel.h), JSON.stringify({ baseline, after })).toBeLessThanOrEqual(
    30,
  );
  expect(after.blankBelowCanvas).toBeLessThanOrEqual(10);
  expect(after.host.scrollW - after.host.clientW).toBeLessThanOrEqual(2);
});

// ---------------------------------------------------------------------------
// Expand / scroll / output glitches
// ---------------------------------------------------------------------------

test("HARSH: expand/collapse must not leave blank band, cover, or overflow", async ({ page }) => {
  await openTerminal(page);
  await page.getByRole("button", { name: "Expand terminal" }).click();
  await expect(terminalPanel(page)).toHaveClass(/is-expanded/);
  await page.waitForTimeout(300);
  let g = await geo(page);
  expect(g.blankBelowCanvas, `expanded blank ${g.blankBelowCanvas}`).toBeLessThanOrEqual(12);
  expect(g.coverCenter === "canvas" || g.coverCenter === "host", `expanded cover ${g.coverCenter}`).toBe(
    true,
  );
  expect(g.panel.r).toBeLessThanOrEqual(g.viewport.w + 1);

  await page.getByRole("button", { name: "Expand terminal" }).click();
  await expect(terminalPanel(page)).not.toHaveClass(/is-expanded/);
  await page.waitForTimeout(300);
  g = await geo(page);
  expect(g.blankBelowCanvas).toBeLessThanOrEqual(10);
  expect(g.hostToKeys ?? 99).toBeLessThanOrEqual(10);
});

test("HARSH: scrollback + new output must not yank layout or cover PTY", async ({ page }) => {
  await openTerminal(page);
  let chunk = "";
  for (let i = 0; i < 80; i++) chunk += `row-${i}\r\n`;
  await emitOutput(page, chunk);
  await page.waitForTimeout(100);

  const host = terminalPanel(page).locator(".terminal-host");
  await host.evaluate((el) => {
    for (let i = 0; i < 20; i++) {
      el.dispatchEvent(
        new WheelEvent("wheel", {
          deltaY: -4,
          deltaMode: WheelEvent.DOM_DELTA_LINE,
          bubbles: true,
          cancelable: true,
        }),
      );
    }
  });
  await page.waitForTimeout(80);
  const before = await geo(page);

  await emitOutput(page, "NEW-OUTPUT-LINE-SHOULD-NOT-JUMP-LAYOUT\r\n");
  await page.waitForTimeout(120);
  const after = await geo(page);

  expect(Math.abs(after.panel.h - before.panel.h), "layout jump on output while scrolled").toBeLessThanOrEqual(
    8,
  );
  expect(Math.abs(after.canvas.t - before.canvas.t), "canvas top jump on output").toBeLessThanOrEqual(8);
  expect(after.coverCenter === "canvas" || after.coverCenter === "host", after.coverCenter).toBe(true);

  // New output pill must not sit off-screen if visible.
  if (after.newOut) {
    expect(after.newOut.r).toBeLessThanOrEqual(after.viewport.w + 1);
    expect(after.newOut.t).toBeGreaterThanOrEqual(-1);
  }
});

test("HARSH: landscape + keyboard must keep PTY filled and keys attached", async ({ page }) => {
  await openTerminal(page, { width: 844, height: 390 });
  let g = await geo(page);
  expect(g.blankBelowCanvas, `landscape blank ${g.blankBelowCanvas}`).toBeLessThanOrEqual(10);
  expect(g.visualFillY, `landscape fillY ${g.visualFillY}`).toBeGreaterThan(0.85);
  expect(g.hostToKeys ?? 99).toBeLessThanOrEqual(10);

  await setKeyboard(page, true, 20, 240);
  g = await geo(page);
  expect(g.keys).not.toBeNull();
  expect(g.band.bottom - g.keys!.b).toBeLessThanOrEqual(16);
  expect(g.panel.h).toBeGreaterThan(140);
  expect(g.coverCenter === "canvas" || g.coverCenter === "host", g.coverCenter).toBe(true);
  expect(g.blankBelowCanvas).toBeLessThanOrEqual(12);
});

test("HARSH: tapping the PTY must accept typed input (Ghostty focus path)", async ({ page }) => {
  await openTerminal(page);
  const beforeFrames = await page.evaluate(
    () => (window as unknown as { __terminalFrames: unknown[] }).__terminalFrames.length,
  );

  // Ghostty focuses a contentEditable scale-layer (not the tiny textarea).
  await canvas(page).click({ position: { x: 40, y: 40 } });
  await page.waitForTimeout(100);
  await page.keyboard.type("abc");
  await page.waitForTimeout(200);

  const frames = await page.evaluate(
    () => (window as unknown as { __terminalFrames: Array<{ type?: string; data?: string }> }).__terminalFrames,
  );
  const typed = frames.filter((f) => f.type === "input").map((f) => f.data).join("");
  expect(typed, `canvas tap did not deliver keystrokes (frames=${JSON.stringify(frames.slice(beforeFrames))})`).toContain(
    "abc",
  );

  await setKeyboard(page, true, 40, 450);
  const open = await geo(page);
  expect(open.panel.h, "focused+keyboard collapsed the terminal").toBeGreaterThan(200);
  expect(open.keys).not.toBeNull();
  expect(open.band.bottom - open.keys!.b, "keys detached after focus+keyboard").toBeLessThanOrEqual(16);
});

test("HARSH: Paste and Hide keyboard must be visible without horizontal pan (iPhone width)", async ({
  page,
}) => {
  await openTerminal(page);
  // Soft keyboard open is when Hide keyboard matters most — and when Paste is
  // the documented iOS clipboard path.
  await setKeyboard(page, true, 48, 420);

  const keys = await page.evaluate(() => {
    const row = document.querySelector(
      "[data-testid='task-terminal-panel'] .terminal-keys",
    ) as HTMLElement;
    const rr = row.getBoundingClientRect();
    return [...row.querySelectorAll("button")].map((b) => {
      const r = b.getBoundingClientRect();
      const label = (b.getAttribute("aria-label") || b.textContent || "").trim();
      const fullyInRow =
        r.left >= rr.left - 1 &&
        r.right <= rr.right + 1 &&
        r.left >= -1 &&
        r.right <= window.innerWidth + 1;
      return { label, fullyInRow, left: r.left, right: r.right, rowRight: rr.right };
    });
  });

  for (const need of ["Paste", "Hide keyboard"]) {
    const hit = keys.find((k) => k.label === need || k.label.includes(need));
    expect(hit, `${need} missing from key row`).toBeTruthy();
    expect(
      hit!.fullyInRow,
      `${need} clipped off-screen (need horizontal pan of a scrollbar-less row): ${JSON.stringify(hit)}`,
    ).toBe(true);
  }
});

test("HARSH: narrow phone must not leave a blank band under the scaled canvas", async ({ page }) => {
  await openTerminal(page, { width: 320, height: 568 });
  const g = await geo(page);
  expect(g.blankBelowCanvas, `narrow blankBelow=${g.blankBelowCanvas} fillY=${g.visualFillY}`).toBeLessThanOrEqual(
    8,
  );
  expect(g.visualFillY, `narrow fillY=${g.visualFillY}`).toBeGreaterThan(0.96);
});
