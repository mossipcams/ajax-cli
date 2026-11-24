import { test, expect, type Page } from "@playwright/test";
import {
  COCKPIT_FIXTURE,
  DETAIL_FIXTURE,
  failLatestTerminalSocket,
  mockFetch,
  mockTerminalWebSocket,
  sessionSnapshotJson,
  waitForTerminalSocket,
} from "./fixtures";

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
        el.closest('[data-testid="task-details-sheet"]') ||
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

type OutletFillLayout = {
  ok: boolean;
  flexGrow: string;
  outletHeight: number;
  routeScrollClientHeight: number;
  heightDelta: number;
};

async function probeOutletFillsRouteScroll(
  page: Page,
  outletKind: "task" | "session",
): Promise<OutletFillLayout> {
  return page.evaluate((kind) => {
    const routeScroll = document.querySelector('[data-testid="route-scroll"]');
    const outlet = document.querySelector(`[data-outlet="${kind}"]`);
    if (!routeScroll || !outlet) {
      return {
        ok: false,
        flexGrow: "",
        outletHeight: 0,
        routeScrollClientHeight: 0,
        heightDelta: Number.POSITIVE_INFINITY,
      };
    }
    const routeStyle = getComputedStyle(routeScroll);
    const outletStyle = getComputedStyle(outlet);
    const paddingBlock =
      Number.parseFloat(routeStyle.paddingTop) + Number.parseFloat(routeStyle.paddingBottom);
    const routeScrollContentHeight = routeScroll.clientHeight - paddingBlock;
    const outletHeight = Math.round(outlet.getBoundingClientRect().height);
    return {
      ok: true,
      flexGrow: outletStyle.flexGrow,
      outletHeight,
      routeScrollClientHeight: routeScrollContentHeight,
      heightDelta: Math.abs(outletHeight - routeScrollContentHeight),
    };
  }, outletKind);
}

function expectOutletFillsRouteScroll(layout: OutletFillLayout, outletKind: "task" | "session") {
  expect(layout.ok, `${outletKind} outlet and route-scroll present`).toBe(true);
  if (!layout.ok) return;
  expect(layout.flexGrow, `${outletKind} outlet flex-grow`).toBe("1");
  expect(
    layout.heightDelta,
    `${outletKind} outlet height ${layout.outletHeight} vs route-scroll ${layout.routeScrollClientHeight}`,
  ).toBeLessThanOrEqual(2);
}


test("dashboard has exactly one normal route scroll owner", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  const { routeScrollCount, rogueOwners } = await probeNormalRouteScrollOwners(page);
  expect(routeScrollCount, "route-scroll elements").toBe(1);
  expect(rogueOwners, "unexpected extra scroll owners").toEqual([]);
});

test("task workspace cross-slide wrappers are display:contents at rest", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator('[data-testid="task-terminal-panel"]')).toBeVisible({
    timeout: 10_000,
  });

  const layout = await page.evaluate(() => {
    const host = document.querySelector(".page-cross-slide-host");
    const pane = document.querySelector(".page-cross-slide-pane");
    const outlet = document.querySelector('[data-outlet="task"]');
    if (!host || !pane || !outlet) return { ok: false as const };

    return {
      ok: true as const,
      hostDisplay: getComputedStyle(host).display,
      paneDisplay: getComputedStyle(pane).display,
      hostIdle: host.classList.contains("page-cross-slide-host-idle"),
      paneIdle: pane.classList.contains("page-cross-slide-pane-idle"),
    };
  });

  expect(layout.ok).toBe(true);
  if (!layout.ok) return;
  expect(layout.hostIdle).toBe(true);
  expect(layout.paneIdle).toBe(true);
  expect(layout.hostDisplay).toBe("contents");
  expect(layout.paneDisplay).toBe("contents");
});

test("task outlet fills route-scroll despite cross-slide wrappers", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);

  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator('[data-testid="task-terminal-panel"]')).toBeVisible({
    timeout: 10_000,
  });

  const layout = await probeOutletFillsRouteScroll(page, "task");
  expectOutletFillsRouteScroll(layout, "task");
});

test("session outlet fills route-scroll despite cross-slide wrappers", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 700 });
  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, session_capable: true, agent: "Cursor" },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.send(sessionSnapshotJson({ cursor: 0, model: "auto", turnState: "idle" }));
  });

  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible({ timeout: 10_000 });

  const layout = await probeOutletFillsRouteScroll(page, "session");
  expectOutletFillsRouteScroll(layout, "session");
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

  const detailsTrigger = page.getByTestId("task-meta-details-trigger");
  await detailsTrigger.click();
  const detailsSheet = page.getByTestId("task-details-sheet");
  await expect(detailsSheet).toBeVisible();

  const detailsBody = page.getByTestId("session-details-body");
  const scrollDims = await detailsBody.evaluate((el) => ({
    scrollHeight: el.scrollHeight,
    clientHeight: el.clientHeight,
  }));
  expect(scrollDims.scrollHeight).toBeGreaterThan(scrollDims.clientHeight + 1);

  await detailsBody.evaluate((el) => {
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

const PHONE_LANDSCAPE_VIEWPORTS = [
  { width: 734, height: 343 },
  { width: 844, height: 390 },
];

const TERMINAL_KEY_LABELS = [
  "Escape",
  "Tab",
  "Left arrow",
  "Up arrow",
  "Down arrow",
  "Right arrow",
  "Control modifier",
  "Paste",
  "Backspace",
  "Start voice input",
];

async function openTaskTerminalAt(page: Page, width: number, height: number) {
  await page.setViewportSize({ width, height });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator('[data-testid="task-terminal-panel"]')).toBeVisible({
    timeout: 10_000,
  });
  await waitForTerminalSocket(page);
  await expect(page.locator('[data-testid="terminal-status"]')).toHaveClass(/is-empty/);
}

async function terminalLandscapeGeometry(page: Page) {
  return page.evaluate(() => {
    const boxOf = (selector: string) => {
      const el = document.querySelector(selector);
      if (!el) return null;
      const box = el.getBoundingClientRect();
      return { top: box.top, bottom: box.bottom, height: box.height };
    };
    const keys = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="terminal-bottom-controls"] .terminal-keys button',
      ),
    ).map((key) => {
      const box = key.getBoundingClientRect();
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
      return {
        label: key.getAttribute("aria-label") ?? key.textContent ?? "",
        hitsKey: !!hit && (hit === key || key.contains(hit)),
        hitTag: hit ? (hit.getAttribute("aria-label") ?? hit.tagName) : "none",
      };
    });
    const row = document.querySelector(".xterm-rows > div");
    return {
      rowHeight: row ? row.getBoundingClientRect().height : 0,
      panel: boxOf('[data-testid="task-terminal-panel"]'),
      wrap: boxOf(".terminal-interaction-wrap"),
      toolbar: boxOf('[data-testid="terminal-bottom-controls"] .terminal-keys'),
      nav: boxOf(".bottom-nav"),
      keys,
    };
  });
}

for (const { width, height } of PHONE_LANDSCAPE_VIEWPORTS) {
  test(`1623 phone landscape ${width}x${height} keeps terminal rows and every key reachable`, async ({
    page,
  }, testInfo) => {
    test.skip(testInfo.project.name !== "mobile-webkit", "phone landscape is mobile-webkit only");
    await openTaskTerminalAt(page, width, height);

    const geometry = await terminalLandscapeGeometry(page);
    expect(geometry.panel, "terminal panel").not.toBeNull();
    expect(geometry.wrap, "terminal interaction wrap").not.toBeNull();
    expect(geometry.toolbar, "terminal key toolbar").not.toBeNull();
    expect(geometry.nav, "bottom nav").not.toBeNull();
    if (!geometry.panel || !geometry.wrap || !geometry.toolbar || !geometry.nav) return;

    expect(geometry.rowHeight, "xterm row rendered").toBeGreaterThan(0);
    expect(
      Math.floor(geometry.wrap.height / geometry.rowHeight),
      "visible terminal rows",
    ).toBeGreaterThanOrEqual(4);
    expect(geometry.toolbar.bottom, "toolbar stays inside terminal panel").toBeLessThanOrEqual(
      geometry.panel.bottom + 1,
    );
    expect(geometry.toolbar.bottom, "toolbar stays above bottom nav").toBeLessThanOrEqual(
      geometry.nav.top + 1,
    );

    const routeScroll = page.locator('[data-testid="route-scroll"]');
    const scrollDims = await routeScroll.evaluate((el) => ({
      scrollHeight: el.scrollHeight,
      clientHeight: el.clientHeight,
    }));
    expect(scrollDims.scrollHeight, "route-scroll overflows for task details").toBeGreaterThan(
      scrollDims.clientHeight + 1,
    );
    await routeScroll.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    const detailsReachable = await page.getByTestId("task-meta-details-trigger").evaluate((el) => {
      const box = el.getBoundingClientRect();
      const nav = document.querySelector(".bottom-nav")?.getBoundingClientRect();
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
      return {
        hitsTrigger: !!hit && (hit === el || el.contains(hit)),
        hitTag: hit ? (hit.getAttribute("data-testid") ?? hit.className) : "none",
        clearOfNav: !nav || box.bottom <= nav.top + 1,
      };
    });
    expect(detailsReachable.hitsTrigger, `Task details hit (${detailsReachable.hitTag})`).toBe(true);
    expect(detailsReachable.clearOfNav, "Task details above bottom nav after scroll").toBe(true);

    expect(geometry.keys.map((key) => key.label)).toEqual(TERMINAL_KEY_LABELS);
    for (const key of geometry.keys) {
      expect(key.hitsKey, `${key.label} hit-tests to itself (got ${key.hitTag})`).toBe(true);
    }

    await failLatestTerminalSocket(page, "tmux session missing");
    const reconnect = page.getByRole("button", { name: "Reconnect" });
    await expect(reconnect).toBeVisible();
    const reconnectHit = await reconnect.evaluate((el) => {
      const box = el.getBoundingClientRect();
      const nav = document.querySelector(".bottom-nav")?.getBoundingClientRect();
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
      return {
        inViewport: box.top >= 0 && box.bottom <= window.innerHeight,
        clearOfNav: !nav || box.bottom <= nav.top + 1,
        hits: !!hit && (hit === el || el.contains(hit)),
      };
    });
    expect(reconnectHit.inViewport, "reconnect control inside viewport").toBe(true);
    expect(reconnectHit.clearOfNav, "reconnect control above bottom nav").toBe(true);
    expect(reconnectHit.hits, "reconnect control hit-tests to itself").toBe(true);
  });
}

test("1623 Paste key is not clipped at 320px portrait", async ({ page }) => {
  await openTaskTerminalAt(page, 320, 568);

  const measured = await page.evaluate(() => {
    const toolbar = document.querySelector('[data-testid="terminal-bottom-controls"] .terminal-keys');
    if (!toolbar) return null;
    const toolbarBox = toolbar.getBoundingClientRect();
    return {
      toolbar: { left: toolbarBox.left, right: toolbarBox.right, top: toolbarBox.top, bottom: toolbarBox.bottom },
      keys: Array.from(toolbar.querySelectorAll<HTMLElement>("button")).map((key) => {
        const box = key.getBoundingClientRect();
        return {
          label: key.getAttribute("aria-label") ?? key.textContent ?? "",
          clipped: key.scrollWidth > key.clientWidth + 1,
          left: box.left,
          right: box.right,
          top: box.top,
          bottom: box.bottom,
        };
      }),
    };
  });
  expect(measured).not.toBeNull();
  if (!measured) return;

  const paste = measured.keys.find((key) => key.label === "Paste");
  expect(paste, "Paste key present").toBeDefined();
  if (!paste) return;
  expect(paste.clipped, "Paste label fits its key box").toBe(false);
  expect(paste.left).toBeGreaterThanOrEqual(measured.toolbar.left);
  expect(paste.right).toBeLessThanOrEqual(measured.toolbar.right);
  expect(paste.left).toBeGreaterThanOrEqual(0);
  expect(paste.right).toBeLessThanOrEqual(320);
  for (const key of measured.keys) {
    expect(key.clipped, `${key.label} label fits its key box`).toBe(false);
  }
});
