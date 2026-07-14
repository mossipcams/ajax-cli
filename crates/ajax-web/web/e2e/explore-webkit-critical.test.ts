// Critical WebKit QA hunts — breakage only.
// Not HIG padding, not console-noise. Fail = operator cannot safely complete a
// core flow, taps the wrong thing, or leaves the UI stuck.

import { test, expect, type Page } from "@playwright/test";
import {
  COCKPIT_FIXTURE,
  DETAIL_FIXTURE,
  mockFetch,
  mockTerminalWebSocket,
  terminalFrames,
  terminalPanel,
  waitForTerminalSocket,
} from "./fixtures";

test.beforeEach(async ({}, testInfo) => {
  test.skip(testInfo.project.name !== "mobile-webkit", "critical WebKit hunts only");
});

const dropAction = {
  action: "drop",
  label: "Drop",
  destructive: true,
  confirmation_required: true,
};

function twoTaskCockpit() {
  return {
    ...COCKPIT_FIXTURE,
    cards: [
      {
        id: "web/fix-login",
        qualified_handle: "web/fix-login",
        repo: "web",
        title: "Fix login",
        status: "waiting",
        status_explanation: "Waiting",
        actions: [dropAction],
      },
      {
        id: "api/add-auth",
        qualified_handle: "api/add-auth",
        repo: "api",
        title: "Add auth",
        status: "running",
        status_explanation: null,
        actions: [dropAction],
      },
    ],
    inbox: {
      items: [
        { task_handle: "web/fix-login", severity: 2 },
        { task_handle: "api/add-auth", severity: 1 },
      ],
    },
  };
}

function detailFor(handle: string, title: string) {
  return {
    ...DETAIL_FIXTURE,
    qualified_handle: handle,
    repo: handle.split("/")[0],
    title,
    actions: [
      { action: "review", label: "Review", destructive: false, confirmation_required: false },
      dropAction,
    ],
  };
}

async function openTask(page: Page, handle: string) {
  await page.goto(`/app.html#/t/${encodeURIComponent(handle)}`);
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });
  await expect(page.locator(`[data-testid='outlet-task'][data-handle='${handle}']`)).toBeVisible();
}

/** What you see at the expand button center must be the expand control. */
async function expandHitProbe(page: Page) {
  return page.evaluate(() => {
    const btn = document.querySelector(
      "[data-testid='task-terminal-panel'] .terminal-expand-corner",
    ) as HTMLElement | null;
    if (!btn) return { ok: false, reason: "expand button missing" };
    const br = btn.getBoundingClientRect();
    const cx = br.left + br.width / 2;
    const cy = br.top + br.height / 2;
    const at = document.elementFromPoint(cx, cy) as HTMLElement | null;
    const hitIsExpand =
      !!at &&
      (at === btn ||
        at.closest?.(".terminal-expand-corner") === btn ||
        at.getAttribute?.("aria-label") === "Expand terminal");
    return {
      ok: hitIsExpand,
      reason: hitIsExpand
        ? ""
        : `expand visual center hit ${at?.tagName}.${String(at?.className).slice(0, 60)}`,
      btnRight: br.right,
      viewportW: window.innerWidth,
      inViewport: br.right <= window.innerWidth + 1 && br.left >= -1,
    };
  });
}

// ---------------------------------------------------------------------------
// CRITICAL: destructive confirm must not carry across task identity
// ---------------------------------------------------------------------------

test("CRITICAL: Drop confirm on task A must not one-tap Drop task B", async ({ page }) => {
  await page.addInitScript(() => {
    (window as unknown as { __ajaxOps: unknown[] }).__ajaxOps = [];
  });
  await page.addInitScript(
    ({ detailA, detailB, cockpit }) => {
      const real = globalThis.fetch.bind(globalThis);
      globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
        const url =
          typeof input === "string"
            ? input
            : input instanceof URL
              ? input.href
              : (input as Request).url;
        const path = new URL(url, "http://localhost").pathname;
        if (path === "/api/cockpit") {
          return new Response(JSON.stringify(cockpit), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }
        if (path === "/api/version") {
          return new Response(JSON.stringify({ version: "0.20.5" }), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }
        if (path === "/api/operations") {
          try {
            const body = JSON.parse(String(init?.body ?? "{}")) as {
              task_handle?: string;
              action?: string;
            };
            (
              window as unknown as { __ajaxOps: Array<{ handle: string; action: string }> }
            ).__ajaxOps.push({
              handle: body.task_handle ?? "",
              action: body.action ?? "",
            });
          } catch {
            /* ignore */
          }
          return new Response(JSON.stringify({ cockpit, output: "ok", error: null }), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }
        const taskMatch = path.match(/^\/api\/tasks\/([^/]+)$/);
        if (taskMatch) {
          const handle = decodeURIComponent(taskMatch[1]!);
          const detail = handle === "api/add-auth" ? detailB : detailA;
          return new Response(JSON.stringify(detail), {
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
        if (path.startsWith("/api/")) {
          return new Response("{}", {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }
        return real(input, init);
      };
    },
    {
      detailA: detailFor("web/fix-login", "Fix login"),
      detailB: detailFor("api/add-auth", "Add auth"),
      cockpit: twoTaskCockpit(),
    },
  );
  await mockTerminalWebSocket(page);

  await openTask(page, "web/fix-login");
  await page.locator("[data-action='drop']").click();
  await expect(page.locator(".action.confirming")).toBeVisible();
  await expect(page.locator("[data-action='drop']")).toHaveAttribute(
    "data-task",
    "web/fix-login",
  );

  // Switch tasks without unmounting the task outlet (hash change, same route kind).
  await page.evaluate(() => {
    location.hash = "#/t/api%2Fadd-auth";
  });
  await expect(page.locator("[data-testid='outlet-task']")).toHaveAttribute(
    "data-handle",
    "api/add-auth",
    { timeout: 10_000 },
  );
  await expect(page.locator("[data-action='drop']")).toHaveAttribute("data-task", "api/add-auth");

  // CRITICAL: must NOT still be in confirming state for the new task.
  await expect(
    page.locator(".action.confirming"),
    "Drop confirm leaked from web/fix-login onto api/add-auth",
  ).toHaveCount(0);

  // First tap on B must arm confirm only — must not POST drop.
  await page.locator("[data-action='drop']").click();
  await expect(page.locator(".action.confirming")).toBeVisible();
  const opsAfterFirst = await page.evaluate(
    () =>
      (window as unknown as { __ajaxOps: Array<{ handle: string; action: string }> }).__ajaxOps,
  );
  expect(
    opsAfterFirst.filter((o) => o.action === "drop"),
    `Drop executed without confirm on task switch: ${JSON.stringify(opsAfterFirst)}`,
  ).toEqual([]);
});

// ---------------------------------------------------------------------------
// CRITICAL: expand visual hit target after scale + rotation
// ---------------------------------------------------------------------------

test("CRITICAL: expand remains the real hit target after scale and rotation", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await openTask(page, "web/fix-login");
  await expect(terminalPanel(page).locator("canvas:not([aria-hidden='true'])")).toBeVisible({
    timeout: 10_000,
  });
  await waitForTerminalSocket(page);
  await page.waitForTimeout(400);

  let probe = await expandHitProbe(page);
  expect(probe.inViewport, JSON.stringify(probe)).toBe(true);
  expect(probe.ok, probe.reason).toBe(true);

  await page.setViewportSize({ width: 844, height: 390 });
  await page.waitForTimeout(400);
  probe = await expandHitProbe(page);
  expect(probe.inViewport, `landscape: ${JSON.stringify(probe)}`).toBe(true);
  expect(probe.ok, `landscape: ${probe.reason}`).toBe(true);

  // Actual click must expand (not a dead visual).
  await page.locator(".terminal-expand-corner").click({ force: false });
  await expect(terminalPanel(page)).toHaveClass(/is-expanded/, { timeout: 3_000 });
});

// ---------------------------------------------------------------------------
// CRITICAL: expand + keyboard must not leave chrome permanently gone
// ---------------------------------------------------------------------------

test("CRITICAL: expand then keyboard-open then collapse restores chrome and navigation", async ({
  page,
}) => {
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await openTask(page, "web/fix-login");
  await waitForTerminalSocket(page);

  await page.getByRole("button", { name: "Expand terminal" }).click();
  await expect(page.locator("html")).toHaveClass(/terminal-expanded/);
  await expect(page.locator(".bottom-nav")).toBeHidden();

  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty("--app-height", "390px");
    document.documentElement.style.setProperty("--app-top", "80px");
  });

  await page.getByRole("button", { name: "Expand terminal" }).click();
  await expect(page.locator("html")).not.toHaveClass(/terminal-expanded/);

  await page.evaluate(() => {
    document.documentElement.classList.remove("keyboard-open");
    document.documentElement.style.removeProperty("--app-height");
    document.documentElement.style.removeProperty("--app-top");
  });

  await expect(page.locator(".bottom-nav")).toBeVisible();
  await expect(page.locator(".cockpit-chrome")).toBeVisible();

  // Must be able to leave the task.
  await page.getByRole("button", { name: "← Back" }).click();
  await expect(page.locator("[data-testid='outlet-dashboard']")).toBeVisible({ timeout: 5_000 });
  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  await expect(page.locator("[data-testid='new-task-sheet']")).toBeVisible();
});

// ---------------------------------------------------------------------------
// CRITICAL: new-task Start must stay usable under keyboard band
// ---------------------------------------------------------------------------

test("CRITICAL: Start remains hittable inside the keyboard band", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  await page.locator(".bottom-nav [data-bottom-action='new-task']").click();
  const sheet = page.locator("[data-testid='new-task-sheet']");
  await expect(sheet).toBeVisible();
  await page.locator("#new-task-title-input").fill("critical hunt");
  await page.locator("#new-task-title-input").click();

  await page.evaluate(() => {
    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty("--app-height", "400px");
    document.documentElement.style.setProperty("--app-top", "50px");
    document.documentElement.style.setProperty("--app-band-height", "400px");
    document.documentElement.style.setProperty("--app-band-top", "50px");
  });

  const start = sheet.getByRole("button", { name: "Start" });
  await expect(start).toBeVisible();
  const box = await start.boundingBox();
  expect(box, "Start missing").not.toBeNull();
  expect(box!.y, "Start above keyboard band").toBeGreaterThanOrEqual(50);
  expect(box!.y + box!.height, "Start buried under keyboard / off band").toBeLessThanOrEqual(
    50 + 400,
  );

  const hit = await start.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const at = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !!at && (at === el || el.contains(at) || (at as HTMLElement).closest?.("button") === el);
  });
  expect(hit, "Start visible but not the hit target under keyboard").toBe(true);
});

// ---------------------------------------------------------------------------
// CRITICAL: terminal input still works after expand/collapse cycle
// ---------------------------------------------------------------------------

test("CRITICAL: toolbar input still reaches the socket after expand/collapse", async ({ page }) => {
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await openTask(page, "web/fix-login");
  await waitForTerminalSocket(page);

  const expand = page.getByRole("button", { name: "Expand terminal" });
  await expand.click();
  await expect(terminalPanel(page)).toHaveClass(/is-expanded/);
  await expand.click();
  await expect(terminalPanel(page)).not.toHaveClass(/is-expanded/);

  await page
    .locator("[data-testid='terminal-bottom-controls']")
    .getByRole("toolbar", { name: "Terminal keys" })
    .getByRole("button", { name: "Esc" })
    .click();

  await expect
    .poll(async () => terminalFrames(page), { timeout: 3_000 })
    .toEqual(expect.arrayContaining([{ type: "input", data: "\x1b" }]));
});
