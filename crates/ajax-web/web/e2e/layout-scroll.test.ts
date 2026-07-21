// Layout/viewport regression suite for Web Cockpit scroll ownership.
// Asserts computed layout (not screenshots). API responses are mocked via
// addInitScript before boot, matching e2e/smoke.test.ts.

import { test, expect, type Page } from "@playwright/test";
import { COCKPIT_FIXTURE, DETAIL_FIXTURE, mockFetch, mockTerminalWebSocket } from "./fixtures";

/** Sane upper bound for a single compact task row (min-height + padding + subline). */
const MAX_TASK_ROW_HEIGHT_PX = 96;

function cockpitWithManyTasks(count: number) {
  const cards = Array.from({ length: count }, (_, index) => ({
    id: `web/task-${index}`,
    qualified_handle: `web/task-${index}`,
    repo: "web",
    title: `Task ${index} with a long title that must stay on one line`,
    status: index % 4 === 0 ? "waiting" : "running",
    status_explanation: index % 4 === 0 ? "Needs review" : null,
    actions: [],
  }));
  return { ...COCKPIT_FIXTURE, cards, inbox: { items: [] } };
}

// ---- layout probes (computed styles, not screenshots) --------------------

type ShellLock = { name: string; overflowY: string; canScroll: boolean };

async function probeLockedShells(page: Page): Promise<ShellLock[]> {
  return page.evaluate(() => {
    const targets: Array<{ name: string; el: Element | null }> = [
      { name: "html", el: document.documentElement },
      { name: "body", el: document.body },
      { name: "#app", el: document.getElementById("app") },
    ];
    return targets.map(({ name, el }) => {
      if (!el) return { name, overflowY: "missing", canScroll: true };
      const style = getComputedStyle(el);
      const overflowY = style.overflowY;
      const canScroll =
        (overflowY === "auto" || overflowY === "scroll") &&
        el.scrollHeight > el.clientHeight + 1;
      return { name, overflowY, canScroll };
    });
  });
}

type ScrollOwner = { selector: string; overflowY: string; scrollHeight: number; clientHeight: number };

async function probeNormalRouteScrollOwners(page: Page): Promise<{
  routeScrollCount: number;
  rogueOwners: ScrollOwner[];
}> {
  return page.evaluate(() => {
    const describe = (el: Element): string => {
      const testId = el.getAttribute("data-testid");
      if (testId) return `[data-testid="${testId}"]`;
      const id = el.id ? `#${el.id}` : "";
      const tag = el.tagName.toLowerCase();
      const cls =
        typeof el.className === "string" && el.className
          ? `.${el.className.trim().split(/\s+/).slice(0, 2).join(".")}`
          : "";
      return `${tag}${id}${cls}`;
    };

    const isExcluded = (el: Element): boolean =>
      !!(
        el.closest('[data-testid="new-task-sheet"]') ||
        el.closest("#new-task-sheet") ||
        el.closest(".sheet-card") ||
        el.closest(".result-output")
      );

    const routeScrollCount = document.querySelectorAll('[data-testid="route-scroll"]').length;
    const rogueOwners: ScrollOwner[] = [];

    for (const el of document.querySelectorAll("*")) {
      if (el.matches('[data-testid="route-scroll"]')) continue;
      if (isExcluded(el)) continue;
      const style = getComputedStyle(el);
      const overflowY = style.overflowY;
      if (overflowY !== "auto" && overflowY !== "scroll") continue;
      if (el.scrollHeight <= el.clientHeight + 1) continue;
      rogueOwners.push({
        selector: describe(el),
        overflowY,
        scrollHeight: el.scrollHeight,
        clientHeight: el.clientHeight,
      });
    }

    return { routeScrollCount, rogueOwners };
  });
}

async function simulateKeyboardBand(page: Page) {
  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty("--app-height", "460px");
    document.documentElement.style.setProperty("--app-top", "40px");
  });
}

async function visibleAppBand(page: Page) {
  return page.evaluate(() => {
    const top = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-top") || "0",
    );
    const height = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue("--app-height") || "0",
    );
    return { top, bottom: top + height };
  });
}

// ---- tests ---------------------------------------------------------------

test("dashboard has exactly one normal route scroll owner", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  const { routeScrollCount, rogueOwners } = await probeNormalRouteScrollOwners(page);
  expect(routeScrollCount, "route-scroll elements").toBe(1);
  expect(rogueOwners, "unexpected extra scroll owners").toEqual([]);
});

test("html, body, and #app never become scroll containers on the dashboard", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  const shells = await probeLockedShells(page);
  for (const shell of shells) {
    expect(shell.overflowY, `${shell.name} overflow-y`).toBe("hidden");
    expect(shell.canScroll, `${shell.name} scrollable`).toBe(false);
  }
});

test("task rows stay within a sane height after many tasks render", async ({ page }) => {
  await mockFetch(page, { "/api/cockpit": cockpitWithManyTasks(40) });
  await page.goto("/app.html");
  await expect(page.getByText("web/task-0")).toBeVisible({ timeout: 10_000 });

  const rowHeights = await page.locator(".task-row").evaluateAll((rows) =>
    rows.map((row) => Math.round(row.getBoundingClientRect().height)),
  );
  expect(rowHeights.length).toBeGreaterThan(10);
  for (const height of rowHeights) {
    expect(height, "task row height").toBeLessThanOrEqual(MAX_TASK_ROW_HEIGHT_PX);
  }
});

test("new task sheet stays inside the simulated keyboard viewport band", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  const sheet = page.locator('[data-testid="new-task-sheet"]');
  await expect(sheet).toBeVisible();
  const titleInput = page.locator("#new-task-title-input");
  await titleInput.click();
  await simulateKeyboardBand(page);

  const band = await visibleAppBand(page);
  const sheetBox = await sheet.boundingBox();
  const inputBox = await titleInput.boundingBox();
  expect(sheetBox).not.toBeNull();
  expect(inputBox).not.toBeNull();

  expect(sheetBox!.y).toBeGreaterThanOrEqual(band.top);
  expect(sheetBox!.y + sheetBox!.height).toBeLessThanOrEqual(band.bottom);
  expect(inputBox!.y).toBeGreaterThanOrEqual(band.top);
  expect(inputBox!.y + inputBox!.height).toBeLessThanOrEqual(band.bottom);
});

const LONG_WORKTREE_PATH =
  "/very/long/repo/path/web__worktrees/ajax-fix-login-with-extra-segments-that-would-overflow-without-shrink";

function tallTaskDetailFixture() {
  return {
    ...DETAIL_FIXTURE,
    worktree_path: LONG_WORKTREE_PATH,
    annotations: [
      "Note one with enough text to grow the meta body on a narrow phone viewport",
      "Note two with enough text to grow the meta body on a narrow phone viewport",
      "Note three with enough text to grow the meta body on a narrow phone viewport",
      "Note four — last annotation must stay reachable via route-scroll",
    ],
    agent_attempts: [
      {
        started_unix_secs: 1_700_000_000,
        completed_unix_secs: 1_700_000_300,
        outcome: "completed",
      },
      {
        started_unix_secs: 1_700_001_000,
        completed_unix_secs: null,
        outcome: "running",
      },
    ],
  };
}

test("open mobile task meta keeps a usable terminal and route-scroll reaches the last note", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await mockFetch(page, { __detail__: tallTaskDetailFixture() });
  await mockTerminalWebSocket(page);

  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator('[data-testid="task-terminal-panel"]')).toBeVisible({
    timeout: 10_000,
  });

  const summary = page.locator(".meta-details summary");
  await summary.click();
  await expect(page.locator(".meta-details[open]")).toBeVisible();

  const routeScroll = page.locator('[data-testid="route-scroll"]');
  const routeOverflow = await routeScroll.evaluate((el) => ({
    scrollWidth: el.scrollWidth,
    clientWidth: el.clientWidth,
  }));
  expect(routeOverflow.scrollWidth).toBeLessThanOrEqual(routeOverflow.clientWidth + 1);

  const terminalHeight = await page
    .locator('[data-testid="task-terminal-panel"]')
    .evaluate((el) => Math.round(el.getBoundingClientRect().height));
  expect(terminalHeight).toBeGreaterThanOrEqual(120);

  const scrollDims = await routeScroll.evaluate((el) => ({
    scrollHeight: el.scrollHeight,
    clientHeight: el.clientHeight,
  }));
  expect(scrollDims.scrollHeight).toBeGreaterThan(scrollDims.clientHeight + 1);

  await routeScroll.evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });

  const activeClearance = await page.evaluate(() => {
    const activeLabel = Array.from(document.querySelectorAll(".detail-grid dt")).find(
      (dt) => dt.textContent?.trim() === "Active",
    );
    const activeValue = activeLabel?.nextElementSibling;
    const bottomNav = document.querySelector(".bottom-nav");
    if (!activeValue || !bottomNav) return { ok: false, activeBottom: 0, navTop: 0 };
    const activeRect = activeValue.getBoundingClientRect();
    const navRect = bottomNav.getBoundingClientRect();
    return {
      ok: activeRect.bottom <= navRect.top + 1,
      activeBottom: activeRect.bottom,
      navTop: navRect.top,
    };
  });
  expect(activeClearance.ok, `Active bottom ${activeClearance.activeBottom} vs nav top ${activeClearance.navTop}`).toBe(
    true,
  );

  const lastNote = page.locator('[data-testid="task-annotations"] li').last();
  await expect(lastNote).toBeInViewport();

  const { routeScrollCount, rogueOwners } = await probeNormalRouteScrollOwners(page);
  expect(routeScrollCount, "route-scroll elements").toBe(1);
  expect(rogueOwners, "unexpected extra scroll owners").toEqual([]);
});
