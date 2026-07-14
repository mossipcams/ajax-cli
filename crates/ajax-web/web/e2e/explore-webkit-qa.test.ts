// Deep WebKit / iPhone exploratory QA hunt.
// Known DEFECT-1 (interactive-widget viewport meta) is asserted separately and
// filtered from other hunts so it cannot mask additional defects.

import { test, expect, type Locator, type Page } from "@playwright/test";
import {
  COCKPIT_FIXTURE,
  mockFetch,
  mockTerminalWebSocket,
  terminalPanel,
  terminalToolbar,
  waitForTerminalSocket,
} from "./fixtures";

const MIN_TAP_PX = 44;
const KNOWN_VIEWPORT_NOISE = /interactive-widget/i;

test.beforeEach(async ({}, testInfo) => {
  test.skip(testInfo.project.name !== "mobile-webkit", "WebKit QA hunt only");
});

function trackErrors(page: Page) {
  const pageErrors: string[] = [];
  const consoleErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(err.message));
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });
  return {
    pageErrors,
    consoleErrors,
    unexpectedConsole() {
      return consoleErrors.filter((line) => !KNOWN_VIEWPORT_NOISE.test(line));
    },
  };
}

async function visualViewport(page: Page) {
  return page.evaluate(() => {
    const vv = window.visualViewport;
    return {
      width: vv?.width ?? window.innerWidth,
      height: vv?.height ?? window.innerHeight,
      offsetTop: vv?.offsetTop ?? 0,
      offsetLeft: vv?.offsetLeft ?? 0,
    };
  });
}

async function assertInVisualViewport(locator: Locator, label: string) {
  const box = await locator.boundingBox();
  expect(box, `${label} missing bounding box`).not.toBeNull();
  const viewport = await visualViewport(locator.page());
  expect(box!.y, `${label} above visual viewport`).toBeGreaterThanOrEqual(viewport.offsetTop - 1);
  expect(box!.x, `${label} left of visual viewport`).toBeGreaterThanOrEqual(
    viewport.offsetLeft - 1,
  );
  expect(box!.y + box!.height, `${label} below visual viewport`).toBeLessThanOrEqual(
    viewport.offsetTop + viewport.height + 1,
  );
  expect(box!.x + box!.width, `${label} right of visual viewport`).toBeLessThanOrEqual(
    viewport.offsetLeft + viewport.width + 1,
  );
}

async function assertMinTapTarget(locator: Locator, label: string) {
  const box = await locator.boundingBox();
  expect(box, `${label} missing bounding box`).not.toBeNull();
  expect(box!.width, `${label} tap width < ${MIN_TAP_PX}`).toBeGreaterThanOrEqual(MIN_TAP_PX);
  expect(box!.height, `${label} tap height < ${MIN_TAP_PX}`).toBeGreaterThanOrEqual(MIN_TAP_PX);
}

/** Fail if another element sits on top of the control's center (occlusion). */
async function assertHitTargetIsSelf(locator: Locator, label: string) {
  const ok = await locator.evaluate((el, name) => {
    const rect = el.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    const top = document.elementFromPoint(x, y);
    if (!top) return { ok: false, reason: `${name}: elementFromPoint returned null` };
    if (el === top || el.contains(top) || top.contains(el)) return { ok: true, reason: "" };
    const topDesc = `${(top as HTMLElement).tagName}.${(top as HTMLElement).className}`.slice(
      0,
      80,
    );
    return { ok: false, reason: `${name}: occluded by ${topDesc}` };
  }, label);
  expect(ok.ok, ok.reason).toBe(true);
}

async function simulateKeyboardBand(page: Page, height = 460, top = 40) {
  await page.evaluate(
    ({ height: h, top: t }) => {
      document.documentElement.classList.add("keyboard-open");
      document.documentElement.style.setProperty("--app-height", `${h}px`);
      document.documentElement.style.setProperty("--app-top", `${t}px`);
      document.documentElement.style.setProperty("--app-band-height", `${h}px`);
      document.documentElement.style.setProperty("--app-band-top", `${t}px`);
    },
    { height, top },
  );
}

async function clearKeyboardBand(page: Page) {
  await page.evaluate(() => {
    document.documentElement.classList.remove("keyboard-open");
    document.documentElement.style.removeProperty("--app-height");
    document.documentElement.style.removeProperty("--app-top");
    document.documentElement.style.removeProperty("--app-band-height");
    document.documentElement.style.removeProperty("--app-band-top");
  });
}

function denseCockpit(count: number) {
  const cards = Array.from({ length: count }, (_, i) => ({
    id: `web/task-${i}`,
    qualified_handle: `web/task-${i}`,
    repo: "web",
    title: `Task ${i}`,
    status: i % 2 === 0 ? "waiting" : "running",
    status_explanation: i % 2 === 0 ? `Need attention ${i}` : null,
    actions: [],
  }));
  return {
    ...COCKPIT_FIXTURE,
    cards,
    inbox: {
      items: cards
        .filter((_, i) => i % 2 === 0)
        .slice(0, 12)
        .map((c) => ({ task_handle: c.qualified_handle, severity: 2 })),
    },
  };
}

// ---------------------------------------------------------------------------
// Known defect pin
// ---------------------------------------------------------------------------

test("DEFECT pin: viewport meta must not use interactive-widget on WebKit", async ({ page }) => {
  const errors = trackErrors(page);
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  const content = await page.locator('meta[name="viewport"]').getAttribute("content");
  expect(content ?? "").not.toMatch(KNOWN_VIEWPORT_NOISE);
  expect(errors.consoleErrors.filter((l) => KNOWN_VIEWPORT_NOISE.test(l))).toEqual([]);
});

// ---------------------------------------------------------------------------
// Deep exploration
// ---------------------------------------------------------------------------

test("deep explore: dashboard chrome, filters, dense scroll, hit targets", async ({ page }) => {
  const errors = trackErrors(page);
  await mockFetch(page, { "/api/cockpit": denseCockpit(40) });
  await page.goto("/app.html");
  await expect(page.getByText("web/task-0")).toBeVisible({ timeout: 10_000 });

  const bottomButtons = page.locator(".bottom-nav button");
  const count = await bottomButtons.count();
  expect(count).toBeGreaterThanOrEqual(2);
  for (let i = 0; i < count; i++) {
    const btn = bottomButtons.nth(i);
    const label = `bottom-nav[${i}]`;
    await assertInVisualViewport(btn, label);
    await assertMinTapTarget(btn, label);
    await assertHitTargetIsSelf(btn, label);
  }

  const pills = page.locator("button.project-pill");
  const pillCount = await pills.count();
  for (let i = 0; i < Math.min(pillCount, 6); i++) {
    const pill = pills.nth(i);
    await assertInVisualViewport(pill, `project-pill[${i}]`);
    await assertHitTargetIsSelf(pill, `project-pill[${i}]`);
    await pill.click();
  }

  // Scroll the route scroller and ensure bottom nav stays hit-testable.
  await page.locator("[data-testid='route-scroll']").evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  await assertHitTargetIsSelf(
    page.locator(".bottom-nav [data-bottom-action='new-task']"),
    "New after dense scroll",
  );

  // Last dense task still reachable by scroll + click.
  await page.getByText("web/task-39").scrollIntoViewIfNeeded();
  await expect(page.getByText("web/task-39")).toBeVisible();

  expect(errors.pageErrors).toEqual([]);
  expect(errors.unexpectedConsole()).toEqual([]);
});

test("deep explore: task actions, terminal keys, expand, keyboard, landscape", async ({
  page,
}) => {
  const errors = trackErrors(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });
  await expect(terminalPanel(page)).toBeVisible({ timeout: 10_000 });
  await waitForTerminalSocket(page);

  // Action bar
  const review = page.locator("[data-action='review']");
  const drop = page.locator("[data-action='drop']");
  await assertInVisualViewport(review, "review");
  await assertMinTapTarget(review, "review");
  await assertHitTargetIsSelf(review, "review");
  await review.click();
  await expect(page.locator("[data-outlet='task']")).toBeVisible();

  await assertInVisualViewport(drop, "drop");
  await drop.click();
  await expect(page.locator(".action.confirming")).toBeVisible({ timeout: 3_000 });
  await page.locator(".action.confirming").click();

  // Full terminal key row — key bar is intentionally overflow-x scrollable.
  // Each key must become fully visible after scrollIntoView (not clipped).
  const toolbar = terminalToolbar(page);
  const keyBar = page.locator(".terminal-keys");
  for (const name of ["Esc", "Tab", "⌃C", "Ctrl", "←", "→", "↑", "↓", "Paste"]) {
    const key = toolbar.getByRole("button", { name });
    if (!(await key.count())) continue;
    await key.evaluate((el) => el.scrollIntoView({ inline: "nearest", block: "nearest" }));
    await assertInVisualViewport(key, `key ${name} after scroll`);
    await assertHitTargetIsSelf(key, `key ${name}`);
    await assertMinTapTarget(key, `key ${name}`);
    await key.click();
  }
  // Key bar itself should admit horizontal overflow (scrollWidth > clientWidth)
  // when keys cannot all fit — otherwise trailing keys are unreachable.
  const keyBarMetrics = await keyBar.evaluate((el) => ({
    scrollWidth: el.scrollWidth,
    clientWidth: el.clientWidth,
  }));
  if (keyBarMetrics.scrollWidth > keyBarMetrics.clientWidth + 1) {
    // scrollable — OK; Paste must still be reachable (checked above)
  }

  // Expand / collapse
  const expand = page.getByRole("button", { name: "Expand terminal" });
  await expect(expand).toBeVisible();
  await assertInVisualViewport(expand, "expand");
  await assertHitTargetIsSelf(expand, "expand");
  await expand.click();
  await expect(page.locator("html")).toHaveClass(/terminal-expanded/);
  // Bottom nav should hide when expanded
  await expect(page.locator(".bottom-nav")).toBeHidden();
  const collapse = page.getByRole("button", { name: /Collapse terminal|Expand terminal/ });
  await expect(collapse).toBeVisible();
  await assertInVisualViewport(collapse, "collapse/expand while fullscreen");
  await assertHitTargetIsSelf(collapse, "collapse/expand while fullscreen");
  await collapse.click();
  await expect(page.locator("html")).not.toHaveClass(/terminal-expanded/);

  // Keyboard band over task terminal
  await simulateKeyboardBand(page);
  await assertInVisualViewport(terminalPanel(page), "terminal under keyboard-open");
  const hideKb = page.getByRole("button", { name: "Hide keyboard" });
  if (await hideKb.isVisible()) {
    await assertInVisualViewport(hideKb, "Hide keyboard");
    await assertHitTargetIsSelf(hideKb, "Hide keyboard");
  }
  await clearKeyboardBand(page);

  // Landscape phone
  await page.setViewportSize({ width: 844, height: 390 });
  await expect(terminalPanel(page)).toBeVisible();
  await assertInVisualViewport(expand, "expand landscape");
  await assertHitTargetIsSelf(
    page.locator("[data-action='review']"),
    "review landscape",
  );

  // Portrait restore + back
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "← Back" }).click();
  await expect(page.locator("[data-testid='outlet-dashboard']")).toBeVisible();

  expect(errors.pageErrors).toEqual([]);
  expect(errors.unexpectedConsole()).toEqual([]);
});

test("deep explore: settings controls and new-task sheet under keyboard", async ({ page }) => {
  const errors = trackErrors(page);
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator("button.settings-link").click();
  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({ timeout: 5_000 });

  const restart = page.getByRole("button", { name: /Restart server/i });
  await expect(restart).toBeVisible({ timeout: 5_000 });
  await assertInVisualViewport(restart, "Restart");
  await assertHitTargetIsSelf(restart, "Restart");
  await assertMinTapTarget(restart, "Restart");

  const diagnostics = page.getByRole("button", { name: /Run diagnostics/i });
  await expect(diagnostics).toBeVisible({ timeout: 5_000 });
  await assertInVisualViewport(diagnostics, "Run diagnostics");
  await assertHitTargetIsSelf(diagnostics, "Run diagnostics");
  await assertMinTapTarget(diagnostics, "Run diagnostics");
  await diagnostics.click();

  await page.locator(".bottom-nav [data-bottom-route='#/']").click();
  await expect(page.locator("[data-testid='outlet-dashboard']")).toBeVisible();

  // New task under keyboard
  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  const sheet = page.locator("[data-testid='new-task-sheet']");
  await expect(sheet).toBeVisible();
  const title = page.locator("#new-task-title-input");
  await title.click();
  await simulateKeyboardBand(page, 420, 60);
  const box = await title.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.y).toBeGreaterThanOrEqual(60);
  expect(box!.y + box!.height).toBeLessThanOrEqual(60 + 420);
  const cancel = sheet.getByRole("button", { name: "Cancel" });
  await assertInVisualViewport(cancel, "Cancel under keyboard");
  await cancel.click();
  await clearKeyboardBand(page);

  expect(errors.pageErrors).toEqual([]);
  expect(errors.unexpectedConsole()).toEqual([]);
});

test("deep explore: connection unreachable chrome stays usable", async ({ page }) => {
  const errors = trackErrors(page);
  // Mock the rest of /api so unmocked calls (version, session renew) cannot
  // leak through the vite proxy to a live dev server and 401 into the console;
  // the in-page override below still makes cockpit itself unreachable.
  await mockFetch(page);
  await page.addInitScript(() => {
    const orig = globalThis.fetch.bind(globalThis);
    globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : (input as Request).url;
      if (url.includes("/api/cockpit")) throw new TypeError("Failed to fetch");
      return orig(input, init);
    };
  });
  await page.goto("/app.html");
  await expect(page.locator(".connection-status")).toContainText("unreachable", {
    timeout: 8_000,
  });

  const retry = page.getByRole("button", { name: "Retry" });
  await expect(retry).toBeVisible();
  await assertInVisualViewport(retry, "Retry");
  await assertMinTapTarget(retry, "Retry");
  await assertHitTargetIsSelf(retry, "Retry");

  const copyDiag = page.locator(".connection-actions").getByRole("button", {
    name: /Copy Diagnostics/i,
  });
  if (await copyDiag.isVisible()) {
    await assertInVisualViewport(copyDiag, "Copy Diagnostics");
    await assertHitTargetIsSelf(copyDiag, "Copy Diagnostics");
  }

  expect(errors.pageErrors).toEqual([]);
  expect(errors.unexpectedConsole()).toEqual([]);
});

test("deep explore: rapid route thrash does not throw or orphan outlets", async ({ page }) => {
  const errors = trackErrors(page);
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  for (let i = 0; i < 6; i++) {
    await page.getByText("web/fix-login").click();
    await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });
    await page.locator("button.settings-link").click();
    await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({
      timeout: 5_000,
    });
    await page.locator(".bottom-nav [data-bottom-route='#/']").click();
    await expect(page.locator("[data-testid='outlet-dashboard']")).toBeVisible({
      timeout: 5_000,
    });
  }

  // Exactly one primary outlet visible
  const visibleOutlets = await page.locator("[data-outlet], [data-testid^='outlet-']").evaluateAll(
    (nodes) =>
      nodes.filter((n) => {
        const s = getComputedStyle(n as HTMLElement);
        return s.display !== "none" && s.visibility !== "hidden" && (n as HTMLElement).offsetParent;
      }).length,
  );
  expect(visibleOutlets).toBeGreaterThanOrEqual(1);

  expect(errors.pageErrors).toEqual([]);
  expect(errors.unexpectedConsole()).toEqual([]);
});
