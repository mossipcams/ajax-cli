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
      send(socket, { type: "message", role: "user", text: "Show the change" });
      send(socket, {
        type: "message", role: "agent",
        text: Array.from({ length: 30 }, (_, index) => `Paragraph ${index} of the answer.`).join("\n\n"),
      });
      send(socket, { type: "turn_end", stopReason: "end_turn" });
    });
  });
});

async function openChat(page: Page) {
  await page.goto("/app.html#/session/web%2Ffix-login");
  await page.getByLabel("Message").fill("Show the change");
  await page.getByLabel("Message").press("Enter");
  await expect(page.getByTestId("session-message-agent")).toBeVisible();
}

async function dragOn(locator: Locator, startX: number, endX: number, y: number) {
  return locator.evaluate(
    (element, points) => {
      const dispatch = (type: string, clientX: number) => {
        const event = new Event(type, { bubbles: true, cancelable: true });
        Object.defineProperty(event, "touches", { value: [{ clientX, clientY: points.y }] });
        Object.defineProperty(event, "changedTouches", { value: [{ clientX, clientY: points.y }] });
        element.dispatchEvent(event);
      };
      return new Promise<string>((resolve) => {
        const engageX = points.startX + Math.sign(points.endX - points.startX) * 60;
        dispatch("touchstart", points.startX);
        dispatch("touchmove", engageX);
        dispatch("touchmove", points.endX);
        requestAnimationFrame(() => requestAnimationFrame(() => {
          const root = element.closest(".session-chat");
          resolve(root ? getComputedStyle(root).transform : "missing");
          dispatch("touchend", points.endX);
        }));
      });
    },
    { startX, endX, y },
  );
}

test("leftward drag across the chat transcript does not move the chat page", async ({ page }) => {
  await openChat(page);
  const hashBefore = new URL(page.url()).hash;
  const transcript = page.getByTestId("session-message-agent");
  const box = await transcript.boundingBox();
  if (!box) throw new Error("transcript missing box");
  const y = box.y + box.height / 2;

  const transformDuringDrag = await dragOn(transcript, 300, 120, y);

  expect(transformDuringDrag).toBe("none");
  await page.waitForTimeout(600);
  expect(await page.getByTestId("session-chat").evaluate((element) => getComputedStyle(element).transform)).toBe("none");
  expect(new URL(page.url()).hash).toBe(hashBefore);
});

test("right swipe from the left edge still navigates back from the chat", async ({ page }) => {
  await openChat(page);
  const transcript = page.getByTestId("session-message-agent");
  const box = await transcript.boundingBox();
  if (!box) throw new Error("transcript missing box");

  await dragOn(transcript, 4, 260, box.y + box.height / 2);

  await expect(page.getByTestId("outlet-dashboard")).toBeVisible({ timeout: 8_000 });
});
