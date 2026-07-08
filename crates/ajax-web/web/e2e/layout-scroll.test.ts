// Layout/viewport regression suite for Web Cockpit scroll ownership.
// Asserts computed layout (not screenshots). API responses are mocked via
// addInitScript before boot, matching e2e/smoke.test.ts.

import { test, expect, type Page } from "@playwright/test";

// ---- fixture data --------------------------------------------------------

const COCKPIT_FIXTURE = {
  backend: { authority: "host-native", control_enabled: true, warning: null },
  repos: { repos: [{ name: "web" }, { name: "api" }] },
  cards: [
    {
      id: "web/fix-login",
      qualified_handle: "web/fix-login",
      repo: "web",
      title: "Fix login",
      status: "waiting",
      status_explanation: "Waiting for review",
      actions: [
        { action: "review", label: "Review", destructive: false, confirmation_required: false },
        { action: "drop", label: "Drop", destructive: true, confirmation_required: true },
      ],
    },
    {
      id: "api/add-auth",
      qualified_handle: "api/add-auth",
      repo: "api",
      title: "Add auth",
      status: "running",
      status_explanation: null,
      actions: [],
    },
  ],
  inbox: { items: [{ task_handle: "web/fix-login", severity: 2 }] },
};

const DETAIL_FIXTURE = {
  qualified_handle: "web/fix-login",
  repo: "web",
  title: "Fix login",
  branch: "ajax/fix-login",
  base_branch: "main",
  worktree_path: "/repo/web/ajax-fix-login",
  tmux_session: "ajax-web-fix-login",
  lifecycle: "reviewable",
  agent: "codex",
  agent_status: "idle",
  status: "waiting",
  status_explanation: "Waiting for review",
  runtime_observation_error: null,
  actions: [
    { action: "review", label: "Review", destructive: false, confirmation_required: false },
    { action: "drop", label: "Drop", destructive: true, confirmation_required: true },
  ],
  live_status_kind: null,
  live_status_summary: null,
  agent_activity: null,
  git: { unpushed_commits: 1 },
  tmux: null,
  annotations: [],
  created_unix_secs: 1700000000,
  last_activity_unix_secs: 1700001000,
  agent_attempts: [],
};

const VERSION_A = { version: "0.20.5" };

/** Sane upper bound for a single compact task row (min-height + padding + subline). */
const MAX_TASK_ROW_HEIGHT_PX = 96;

// ---- fetch mock helper (smoke.test.ts shape) -----------------------------

async function mockFetch(page: Page, extra: Record<string, unknown> = {}) {
  const routes: Record<string, unknown> = {
    "/api/cockpit": COCKPIT_FIXTURE,
    "/api/version": VERSION_A,
    "/api/health": { status: "ok" },
    "/api/operations": { cockpit: COCKPIT_FIXTURE, output: "ok", error: null },
    "/api/server/restart": {},
    __detail__: DETAIL_FIXTURE,
    ...extra,
  };

  await page.addInitScript((routeMap: Record<string, unknown>) => {
    const real = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;

      if (Object.prototype.hasOwnProperty.call(routeMap, path)) {
        return new Response(JSON.stringify(routeMap[path]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (/^\/api\/tasks\/[^/]+\/pane$/.test(path)) {
        return new Response(
          JSON.stringify({ sequence: 0, lines: [], tmux_exists: false, state: null }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (/^\/api\/tasks\/[^/]+$/.test(path)) {
        return new Response(JSON.stringify(routeMap["__detail__"]), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      if (path.startsWith("/api/")) {
        return new Response(JSON.stringify({ error: "not found" }), {
          status: 404,
          headers: { "content-type": "application/json" },
        });
      }
      return real(input, init);
    };
  }, routes);
}

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

async function enableTerminalPlaceholder(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem("ajax.debug.terminalPlaceholder", "true");
  });
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
        el.closest('[data-testid="task-terminal-panel"]') ||
        el.closest('[data-testid="terminal-placeholder"]') ||
        el.closest('[data-testid="new-task-sheet"]') ||
        el.closest("#new-task-sheet") ||
        el.closest(".sheet-card") ||
        el.closest(".result-output") ||
        el.closest(".terminal-keys")
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

test("terminal placeholder proves layout without Ghostty", async ({ page }) => {
  await enableTerminalPlaceholder(page);
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  const placeholder = page.locator('[data-testid="terminal-placeholder"]');
  await expect(placeholder).toBeVisible({ timeout: 10_000 });
  await expect(page.locator("[data-testid='task-terminal-panel'] canvas:not([aria-hidden='true'])")).toHaveCount(0);

  const panel = page.locator("[data-testid='task-terminal-panel']");
  const panelStyle = await panel.evaluate((el) => {
    const style = getComputedStyle(el);
    return {
      overflow: style.overflow,
      minHeight: style.minHeight,
      maxHeight: style.maxHeight,
    };
  });
  expect(panelStyle.overflow).toBe("hidden");
  expect(panelStyle.minHeight).not.toBe("0px");
});

test("task detail route scroll survives expanded terminal close", async ({ page }, testInfo) => {
  await enableTerminalPlaceholder(page);
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator('[data-testid="terminal-placeholder"]')).toBeVisible({
    timeout: 10_000,
  });

  const routeScroll = page.locator('[data-testid="route-scroll"]');
  await expect(routeScroll).toBeVisible();
  await expect(page.locator("[data-testid='task-terminal-panel']")).toBeVisible();

  const isMobileWebkit = testInfo.project.name === "mobile-webkit";

  if (isMobileWebkit) {
    // Mobile task route locks route-scroll (:has([data-outlet="task"])); only
    // the terminal scrolls. Setting scrollTop must leave it at 0.
    const beforeExpand = await routeScroll.evaluate((el) => {
      const maxScroll = el.scrollHeight - el.clientHeight;
      el.scrollTop = Math.max(0, maxScroll);
      return { scrollTop: el.scrollTop, maxScroll };
    });
    expect(beforeExpand.maxScroll, "mobile task route must not overflow").toBeLessThanOrEqual(0);
    expect(beforeExpand.scrollTop, "mobile task route scrollTop stays locked").toBe(0);

    await page.getByRole("button", { name: "Expand terminal" }).click();
    await expect(page.locator("html")).toHaveClass(/terminal-expanded/);

    await page.getByRole("button", { name: "Expand terminal" }).click();
    await expect(page.locator("html")).not.toHaveClass(/terminal-expanded/);

    const afterClose = await routeScroll.evaluate((el) => {
      const maxScroll = el.scrollHeight - el.clientHeight;
      el.scrollTop = Math.max(0, maxScroll);
      return { scrollTop: el.scrollTop, maxScroll };
    });
    expect(afterClose.maxScroll, "route must stay non-scrollable after expand close").toBeLessThanOrEqual(0);
    expect(afterClose.scrollTop, "route scrollTop after expand close").toBe(0);
    return;
  }

  const beforeExpand = await routeScroll.evaluate((el) => {
    el.scrollTop = Math.min(120, el.scrollHeight - el.clientHeight);
    return el.scrollTop;
  });
  expect(beforeExpand).toBeGreaterThan(0);

  await page.getByRole("button", { name: "Expand terminal" }).click();
  await expect(page.locator("html")).toHaveClass(/terminal-expanded/);

  await page.getByRole("button", { name: "Expand terminal" }).click();
  await expect(page.locator("html")).not.toHaveClass(/terminal-expanded/);

  const afterClose = await routeScroll.evaluate((el) => {
    const maxScroll = Math.max(0, el.scrollHeight - el.clientHeight);
    el.scrollTop = maxScroll;
    return { scrollTop: el.scrollTop, maxScroll, canScroll: maxScroll > 0 };
  });
  expect(afterClose.canScroll, "route should remain scrollable").toBe(true);
  expect(afterClose.scrollTop, "route scrollTop after expand close").toBeGreaterThan(0);
});
