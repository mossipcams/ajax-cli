// Visual-regression guard via computed styles. The Svelte migration once shipped
// with styles.css reduced to a stub: components rendered as unstyled grey blocks
// while the text/data-attribute smoke tests stayed green. These tests assert that
// the cockpit's stylesheet is actually applied — they fail loudly if the styling
// ever regresses to browser defaults. Colors are token values from styles.css.
//
// OS-independent on purpose: we assert computed colors/box metrics, not pixel
// screenshots, so there are no platform-specific baselines to maintain.

import { test, expect, type Page, type Locator } from "@playwright/test";

// ---- design tokens (must match styles.css :root) -------------------------

const ACCENT = "rgb(135, 175, 215)"; // --accent (CLI xterm 110)
const WARN = "rgb(215, 175, 95)"; // --warn (CLI xterm 179)
const DANGER = "rgb(215, 135, 135)"; // --danger (CLI xterm 174)
const TRANSPARENT = "rgba(0, 0, 0, 0)";

// ---- fixtures ------------------------------------------------------------

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

// ---- fetch mock (boots before the app, same shape as smoke.test.ts) ------

async function mockFetch(page: Page) {
  await page.addInitScript(({ cockpit, detail }) => {
    globalThis.fetch = async (input: RequestInfo | URL): Promise<Response> => {
      const url =
        typeof input === "string" ? input
        : input instanceof URL ? input.href
        : (input as Request).url;
      const path = new URL(url, "http://localhost").pathname;
      const json = (body: unknown) =>
        new Response(JSON.stringify(body), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      if (path === "/api/cockpit") return json(cockpit);
      if (path === "/api/version") return json({ version: "0.20.5" });
      if (path === "/api/health") return json({ status: "ok" });
      if (/^\/api\/tasks\/[^/]+\/pane$/.test(path))
        return json({ sequence: 0, lines: [], tmux_exists: true, state: null });
      if (/^\/api\/tasks\/[^/]+$/.test(path)) return json(detail);
      return json({});
    };
  }, { cockpit: COCKPIT_FIXTURE, detail: DETAIL_FIXTURE });
}

function bg(locator: Locator) {
  return locator.evaluate((el) => getComputedStyle(el).backgroundColor);
}

// ---- tests ---------------------------------------------------------------

test("dashboard chrome and cards carry the cockpit stylesheet", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  // Bottom-nav "New" button is the accent call-to-action, not a default button.
  const newButton = page.locator('.bottom-nav button[data-bottom-action="new-task"]');
  expect(await bg(newButton)).toBe(ACCENT);

  // Active project pill is filled accent (selection); warn stays for attention.
  const activePill = page.locator(".project-pill.is-active").first();
  expect(await bg(activePill)).toBe(ACCENT);

  // Inbox row: a compact task-row (same shape as the calm list) with a tone
  // (warn for "waiting") left accent instead of separate card chrome.
  const inboxRow = page.locator(".task-row.is-inbox").first();
  const rowStyle = await inboxRow.evaluate((el) => {
    const s = getComputedStyle(el);
    return {
      bg: s.backgroundColor,
      leftWidth: s.borderLeftWidth,
      leftColor: s.borderLeftColor,
    };
  });
  expect(rowStyle.bg).not.toBe(TRANSPARENT);
  expect(rowStyle.leftWidth).toBe("3px");
  expect(rowStyle.leftColor).toBe(WARN);

  // Status label paints with the tone color (waiting -> warn), not default ink.
  const status = page.locator(".task-row-status").first();
  expect(await status.evaluate((el) => getComputedStyle(el).color)).toBe(WARN);

  // Task rows have the compact list padding (would be 0 if unstyled).
  const row = page.locator(".task-row").first();
  expect(await row.evaluate((el) => getComputedStyle(el).paddingTop)).toBe("10px");

  // New-task row is the dashed CTA.
  const newTaskRow = page.locator(".new-task-row");
  expect(await newTaskRow.evaluate((el) => getComputedStyle(el).borderTopStyle)).toBe("dashed");
});

test("task detail panels and action buttons are styled", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name === "mobile-webkit", "desktop panel styling is collapsed on mobile");
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.getByText("Waiting for review")).toBeVisible({ timeout: 10_000 });

  // Primary action (first action) is the filled accent button.
  const primary = page.locator(".action.primary").first();
  expect(await bg(primary)).toBe(ACCENT);

  // Destructive action carries the danger accent color.
  const destructive = page.locator('.action[data-destructive="true"]').first();
  expect(await destructive.evaluate((el) => getComputedStyle(el).color)).toBe(DANGER);

  // Interact panel is a flat hairline strip, not a raised card.
  const panel = page.locator(".interact-panel").first();
  const panelStyle = await panel.evaluate((el) => {
    const s = getComputedStyle(el);
    return { bg: s.backgroundColor, borderTopWidth: s.borderTopWidth };
  });
  expect(panelStyle.bg).toBe(TRANSPARENT);
  expect(panelStyle.borderTopWidth).toBe("1px");

  // Status glyph+label paints with the tone color (waiting -> warn).
  const pill = page.locator(".interact-pill").first();
  expect(await pill.evaluate((el) => getComputedStyle(el).color)).toBe(WARN);

  // Detail title uses the compact mono heading, not default h1.
  const title = page.locator(".detail-title");
  expect(await title.evaluate((el) => getComputedStyle(el).fontSize)).toBe("16px");
});

test("settings view sections are styled", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");
  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({ timeout: 10_000 });

  // Each settings section has a top hairline rule.
  const section = page.locator(".settings-section").first();
  const style = await section.evaluate((el) => {
    const s = getComputedStyle(el);
    return { borderTopWidth: s.borderTopWidth, paddingTop: s.paddingTop };
  });
  expect(style.borderTopWidth).toBe("1px");
  expect(style.paddingTop).toBe("16px");
});
