// Follow-output / scrollback coverage against the real ghostty-web engine.
// Proves that when the operator is reading history, new PTY output must not
// yank the viewport to bottom — surfaced by the "New output ↓" control.

import { test, expect, type Page } from "@playwright/test";
import {
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  waitForTerminalSocket,
} from "./fixtures";

const newOutputButton = (page: Page) =>
  page.getByRole("button", { name: "New output ↓" });

const gridCanvas = (page: Page) =>
  terminalPanel(page).locator("canvas:not([aria-hidden='true'])");

/** Vite HMR also lands in `__terminalSockets`; pick the task-terminal bridge. */
async function emitTerminalOutput(page: Page, text: string) {
  const delivered = await page.evaluate((payload) => {
    const sockets = (
      window as unknown as {
        __terminalSockets: Array<{
          url?: string;
          emitMessage(data: string): void;
        }>;
      }
    ).__terminalSockets;
    const socket = [...sockets]
      .reverse()
      .find((item) => typeof item.url === "string" && item.url.includes("/terminal"));
    if (!socket) return false;
    socket.emitMessage(payload);
    return true;
  }, text);
  expect(delivered).toBe(true);
}

function scrollbackChunk(from: number, count: number): string {
  let out = "";
  for (let i = from; i < from + count; i += 1) {
    out += `row ${i}\r\n`;
  }
  return out;
}

async function openTaskTerminal(page: Page) {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await expect(gridCanvas(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);
}

async function swipeIntoScrollback(page: Page) {
  const canvas = gridCanvas(page);
  const box = await canvas.boundingBox();
  if (!box) throw new Error("terminal canvas has no bounding box");

  // Packet path: same canvas.dragTo used by ios-terminal-smoke (touch projects).
  await canvas.dragTo(canvas, {
    sourcePosition: { x: box.width / 2, y: box.height * 0.8 },
    targetPosition: { x: box.width / 2, y: box.height * 0.2 },
  });

  // Ajax owns scroll via touch/wheel on `.terminal-host`, not mouse-drag on the
  // canvas. Desktop Chromium's dragTo is mouse-only, so also wheel the host
  // (works on both projects; avoids page.mouse.wheel which mobile-webkit rejects).
  const host = terminalPanel(page).locator(".terminal-host");
  await host.evaluate((el) => {
    for (let i = 0; i < 12; i += 1) {
      el.dispatchEvent(
        new WheelEvent("wheel", {
          deltaY: -3,
          deltaMode: WheelEvent.DOM_DELTA_LINE,
          bubbles: true,
          cancelable: true,
        }),
      );
    }
  });
}

test("terminal holds scrollback position when new output arrives", async ({ page }) => {
  await openTaskTerminal(page);

  await emitTerminalOutput(page, scrollbackChunk(0, 200));
  // Follow-output while pinned: control must stay hidden.
  await expect(newOutputButton(page)).not.toBeVisible();

  await swipeIntoScrollback(page);

  await emitTerminalOutput(page, scrollbackChunk(200, 40));

  // Load-bearing: view held scrollback, so unseen-output control appears.
  await expect(newOutputButton(page)).toBeVisible({ timeout: 10_000 });

  await newOutputButton(page).click();
  await expect(newOutputButton(page)).not.toBeVisible();
});

test("New output pill does not shrink terminal host or move bottom controls", async ({
  page,
}) => {
  await openTaskTerminal(page);

  await emitTerminalOutput(page, scrollbackChunk(0, 200));
  await expect(newOutputButton(page)).not.toBeVisible();

  await swipeIntoScrollback(page);

  const host = terminalPanel(page).locator(".terminal-host");
  const bottomControls = terminalPanel(page).locator(
    '[data-testid="terminal-bottom-controls"]',
  );
  await expect(host).toBeVisible();
  await expect(bottomControls).toBeVisible();

  const before = await page.evaluate(() => {
    const panel = document.querySelector('[data-testid="task-terminal-panel"]');
    const hostEl = panel?.querySelector(".terminal-host");
    const controls = panel?.querySelector('[data-testid="terminal-bottom-controls"]');
    if (!hostEl || !controls) return null;
    const hostBox = hostEl.getBoundingClientRect();
    const controlsBox = controls.getBoundingClientRect();
    return { hostHeight: hostBox.height, controlsTop: controlsBox.top };
  });
  expect(before).not.toBeNull();

  await emitTerminalOutput(page, scrollbackChunk(200, 40));
  await expect(newOutputButton(page)).toBeVisible({ timeout: 10_000 });

  const after = await page.evaluate(() => {
    const panel = document.querySelector('[data-testid="task-terminal-panel"]');
    const hostEl = panel?.querySelector(".terminal-host");
    const controls = panel?.querySelector('[data-testid="terminal-bottom-controls"]');
    if (!hostEl || !controls) return null;
    const hostBox = hostEl.getBoundingClientRect();
    const controlsBox = controls.getBoundingClientRect();
    return { hostHeight: hostBox.height, controlsTop: controlsBox.top };
  });
  expect(after).not.toBeNull();

  expect(Math.abs(after!.hostHeight - before!.hostHeight)).toBeLessThanOrEqual(1);
  expect(Math.abs(after!.controlsTop - before!.controlsTop)).toBeLessThanOrEqual(1);
});
