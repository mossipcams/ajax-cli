// Swipe-to-reveal e2e characterization. Pins TaskList behavior across
// implementations: a left touch-drag on a dashboard row opens it by exactly
// SWIPE_REVEAL_WIDTH (88px) and tapping the revealed first action dispatches
// the operation (no second confirm tap for non-destructive review).
//
// Mobile-webkit only — desktop has no touch path and no reduced-pointer
// equivalent, so we skip other projects up front. Touch events are dispatched
// in-page via the same Object.defineProperty(touches) pattern used in
// terminal-behavior.test.ts so the gesture runs against the real action.

import { test, expect, type Page, type Locator } from "@playwright/test";
import { mockFetch } from "./fixtures";

const TARGET_HANDLE = "web/fix-login";
const REVEAL_WIDTH_PX = 88;
const OPERATION_PATH = "/api/operations";

type FetchCall = { url: string; method: string; body: string | null };

async function installFetchSpy(page: Page) {
  await page.addInitScript(() => {
    const calls: Array<{ url: string; method: string; body: string | null }> = [];
    Object.defineProperty(window, "__fetchCalls", {
      value: calls,
      configurable: true,
    });
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : (input as Request).url;
      const method = (init?.method ?? "GET").toUpperCase();
      const body = typeof init?.body === "string" ? init.body : null;
      calls.push({ url, method, body });
      return orig(input, init);
    };
  });
}

const fetchCalls = (page: Page) =>
  page.evaluate(
    () =>
      (window as unknown as { __fetchCalls: FetchCall[] }).__fetchCalls,
  );

async function touchDragRowLeft(page: Page, row: Locator, dx: number) {
  await row.evaluate(async (el, distance) => {
    const rect = el.getBoundingClientRect();
    const startX = rect.left + rect.width * 0.7;
    const startY = rect.top + rect.height / 2;
    const endX = startX - distance;
    const endY = startY;
    const make = (type: string, x: number, y: number) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", {
        value: [{ clientX: x, clientY: y }],
      });
      Object.defineProperty(event, "changedTouches", {
        value: [{ clientX: x, clientY: y }],
      });
      return event;
    };
    el.dispatchEvent(make("touchstart", startX, startY));
    el.dispatchEvent(make("touchmove", endX, endY));
    el.dispatchEvent(
      new Event("touchend", { bubbles: true, cancelable: true }),
    );
  }, dx);
}

// Playwright requires object-destructured fixtures; empty pattern is intentional.
// eslint-disable-next-line no-empty-pattern -- Playwright beforeEach fixture contract
test.beforeEach(({}, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "swipe-reveal is a touch gesture; desktop has no equivalent",
  );
});

test("left swipe opens the row to SWIPE_REVEAL_WIDTH and the revealed action dispatches the operation", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  // mockFetch must install first so the spy wraps it; otherwise the mock
  // returns early for matched paths and the spy never sees the operation POST.
  await mockFetch(page);
  await installFetchSpy(page);
  await page.goto("/app.html");

  const row = page.locator(`.task-row[data-handle="${TARGET_HANDLE}"]`);
  await expect(row).toBeVisible({ timeout: 10_000 });

  // Swipe past the 56px snap trigger so the action settles open at the 88px cap.
  await touchDragRowLeft(page, row, 120);

  // Reveal state: row is flagged is-revealed and translated by exactly 88px.
  await expect(row).toHaveClass(/is-revealed/);
  await expect
    .poll(() => row.evaluate((el) => (el as HTMLElement).style.transform))
    .toBe(`translateX(-${REVEAL_WIDTH_PX}px)`);

  // The revealed action is now visually present in the row wrap and clickable.
  const revealedAction = page.locator(
    `.task-row-wrap[data-handle="${TARGET_HANDLE}"] [data-action="review"]`,
  );
  await expect(revealedAction).toBeVisible();

  await revealedAction.click();

  // The tap must POST to /api/operations — review is non-destructive so it
  // fires immediately without a second confirm tap (matches smoke flow).
  await expect
    .poll(() => fetchCalls(page))
    .toContainEqual(
      expect.objectContaining({
        url: expect.stringContaining(OPERATION_PATH),
        method: "POST",
      }),
    );
});
