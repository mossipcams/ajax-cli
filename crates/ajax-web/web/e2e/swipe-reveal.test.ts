import { test, expect, type Page, type Locator } from "@playwright/test";
import { mockFetch } from "./fixtures";

const TARGET_HANDLE = "api/add-auth";
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
  await mockFetch(page);
  await installFetchSpy(page);
  await page.goto("/app.html");

  const row = page.locator(`.task-row[data-handle="${TARGET_HANDLE}"]`);
  await expect(row).toBeVisible({ timeout: 10_000 });

  const revealWidth = await page
    .locator(`.task-row-wrap[data-handle="${TARGET_HANDLE}"]`)
    .evaluate((el) => {
      const raw = (el as HTMLElement).style.getPropertyValue("--task-row-reveal-width");
      const parsed = parseInt(raw, 10);
      return Number.isFinite(parsed) ? parsed : 158;
    });

  await touchDragRowLeft(page, row, revealWidth + 20);

  await expect(row).toHaveClass(/is-revealed/);
  await expect
    .poll(() => row.evaluate((el) => (el as HTMLElement).style.transform))
    .toBe(`translateX(-${revealWidth}px)`);

  const revealedAction = page.locator(
    `.task-row-wrap[data-handle="${TARGET_HANDLE}"] [data-action="review"]`,
  );
  await expect(revealedAction).toBeVisible();

  await revealedAction.click();

  await expect
    .poll(() => fetchCalls(page))
    .toContainEqual(
      expect.objectContaining({
        url: expect.stringContaining(OPERATION_PATH),
        method: "POST",
      }),
    );
});
