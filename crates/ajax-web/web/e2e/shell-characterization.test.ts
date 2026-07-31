// Shell characterization e2e suite. Pins update-banner visibility/reload and
// dashboard pull-to-refresh. Written against the pre-S7 shell and kept as the
// invariant the React shell must still satisfy.
// API responses are mocked via addInitScript (same pattern as
// smoke.test.ts) so tests run without a live Rust server.

import { test, expect, type Page, type Locator } from "@playwright/test";
import { mockFetch } from "./fixtures";

// Resisted pull distance is raw drag × 0.5; threshold is 64px resisted.
const PULL_RAW_DELTA_PX = 140;

async function installStatefulVersionMock(page: Page) {
  await page.addInitScript(() => {
    // Literal strings match fixtures.VERSION_A / VERSION_B; inlined because
    // addInitScript argument serialization breaks object identity for A vs B.
    const versionA = "0.20.5";
    const versionB = "0.21.0-new";
    let calls = 0;
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (
      input: RequestInfo | URL,
      init?: RequestInit,
    ): Promise<Response> => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;
      if (path === "/api/version") {
        calls += 1;
        const version = calls === 1 ? versionA : versionB;
        return new Response(JSON.stringify({ version }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return orig(input, init);
    };
  });
}

async function installCockpitCallCounter(page: Page) {
  await page.addInitScript(() => {
    let calls = 0;
    Object.defineProperty(window, "__cockpitCalls", {
      get: () => calls,
      configurable: true,
    });
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (
      input: RequestInfo | URL,
      init?: RequestInit,
    ): Promise<Response> => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;
      if (path === "/api/cockpit") calls += 1;
      return orig(input, init);
    };
  });
}

const cockpitCalls = (page: Page) =>
  page.evaluate(
    () => (window as unknown as { __cockpitCalls: number }).__cockpitCalls ?? 0,
  );

async function touchDragDown(target: Locator, dy: number) {
  await target.evaluate((el, distance) => {
    const rect = el.getBoundingClientRect();
    const startX = rect.left + rect.width / 2;
    const startY = rect.top + rect.height * 0.25;
    const endX = startX;
    const endY = startY + distance;
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
    el.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));
  }, dy);
}

test("update banner appears on version change and reloads on tap", async ({
  page,
}) => {
  test.setTimeout(90_000);
  await mockFetch(page);
  await installStatefulVersionMock(page);
  await page.goto("/app.html");

  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  const banner = page.locator("button.update-banner");
  // Boot resume + the 30s dashboard poll both call checkVersion; allow either path.
  await expect(banner).toBeVisible({ timeout: 45_000 });
  await expect(banner).toHaveText("Update ready — tap to reload");

  await Promise.all([page.waitForEvent("load"), banner.click()]);
});

test("pull-to-refresh past threshold reloads the cockpit", async ({
  page,
}, testInfo) => {
  test.skip(
    testInfo.project.name !== "mobile-webkit",
    "pull-to-refresh is a touch gesture; desktop has no equivalent",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await installCockpitCallCounter(page);
  await page.goto("/app.html");

  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  const outlet = page.locator('[data-testid="outlet-dashboard"]');
  await expect(outlet).toBeVisible();

  const baseline = await cockpitCalls(page);
  await touchDragDown(outlet, PULL_RAW_DELTA_PX);

  await expect.poll(() => cockpitCalls(page)).toBeGreaterThan(baseline);
});
