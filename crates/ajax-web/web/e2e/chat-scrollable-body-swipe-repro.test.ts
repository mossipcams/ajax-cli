// Mobile-webkit: overflowing tool and diff bodies own their touch gestures.
//
//   npm run web:smoke -- e2e/chat-scrollable-body-swipe-repro.test.ts

import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  DETAIL_FIXTURE,
  mockFetch,
  sessionEventJson,
  sessionSnapshotJson,
  type SessionServerEvent,
} from "./fixtures";

test.beforeEach(async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "mobile-webkit", "touch regression");
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });

  let cursor = 0;
  const send = (socket: { send: (data: string) => void }, event: SessionServerEvent) =>
    socket.send(sessionEventJson(cursor++, event));
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.send(sessionSnapshotJson({ cursor, model: "auto", turnState: "idle" }));
    socket.onMessage((message) => {
      if (typeof message !== "string" || JSON.parse(message).type !== "prompt") return;
      const lines = Array.from({ length: 80 }, (_, index) => `line ${index}`).join("\n");
      send(socket, { type: "message", role: "user", text: "Show the change" });
      send(socket, {
        type: "tool_call", callId: "output", title: "cargo test", kind: "execute",
        status: "failed", content: [{ type: "text", text: lines }],
      });
      send(socket, {
        type: "tool_call", callId: "diff", title: "Edit config", kind: "edit",
        status: "completed",
        content: [{ type: "diff", path: "src/config.ts", oldText: lines, newText: lines.replaceAll("line", "changed") }],
      });
      send(socket, { type: "turn_end", stopReason: "end_turn" });
    });
  });
});

async function diagonalTouch(locator: Locator) {
  return locator.evaluate((element) => {
    const dispatch = (type: string, x: number, y: number) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", { value: [{ clientX: x, clientY: y }] });
      Object.defineProperty(event, "changedTouches", { value: [{ clientX: x, clientY: y }] });
      element.dispatchEvent(event);
      return event.defaultPrevented;
    };
    dispatch("touchstart", 220, 300);
    const prevented = dispatch("touchmove", 100, 220);
    dispatch("touchend", 100, 220);
    return prevented;
  });
}

async function openTurn(page: Page) {
  await page.goto("/app.html#/session/web%2Ffix-login");
  await page.getByLabel("Message").fill("Show the change");
  await page.getByLabel("Message").press("Enter");
  await expect(page.getByTestId("session-tool-output")).toBeVisible();
  await page.getByTestId("session-tool-output-expand").click();
  await page.getByTestId("session-tool-card").nth(1).getByRole("button").click();
}

test("long tool and diff bodies keep touch scrolling local", async ({ page }) => {
  await openTurn(page);

  for (const body of [page.getByTestId("session-tool-output"), page.locator(".session-diff-body")]) {
    await expect(body).toBeVisible();
    expect(await body.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
    expect(await diagonalTouch(body)).toBe(false);
    await body.evaluate((element) => { element.scrollTop = 120; });
    expect(await body.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    await expect(page.getByTestId("outlet-diff")).toHaveCount(0);
  }
});

test("page swipe outside bodies still opens Diff", async ({ page }) => {
  await openTurn(page);
  const header = page.getByTestId("mobile-chrome-header");
  const box = await header.boundingBox();
  if (!box) throw new Error("header missing box");
  await header.evaluate((element, point) => {
    const dispatch = (type: string, x: number) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", { value: [{ clientX: x, clientY: point.y }] });
      Object.defineProperty(event, "changedTouches", { value: [{ clientX: x, clientY: point.y }] });
      element.dispatchEvent(event);
    };
    dispatch("touchstart", point.x + 120);
    dispatch("touchmove", point.x);
    dispatch("touchend", point.x);
  }, { x: box.x + box.width * 0.15, y: box.y + box.height / 2 });
  await expect(page.getByTestId("outlet-diff")).toBeVisible({ timeout: 8_000 });
});
