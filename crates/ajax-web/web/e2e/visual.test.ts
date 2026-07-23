// Visual-regression guard via computed styles. The Svelte migration once shipped
// with styles.css reduced to a stub: components rendered as unstyled grey blocks
// while the text/data-attribute smoke tests stayed green. These tests assert that
// the cockpit's stylesheet is actually applied — they fail loudly if the styling
// ever regresses to browser defaults. Colors are token values from styles.css.
//
// OS-independent on purpose: we assert computed colors/box metrics, not pixel
// screenshots, so there are no platform-specific baselines to maintain. (CI runs
// WebKit on ubuntu-latest while development happens on macOS, and the design's
// font stack — Avenir Next / Helvetica Neue — does not exist on Linux, so pixel
// baselines could never agree across the two.)
//
// The per-element assertions above catch "the stylesheet stopped applying". They
// cannot catch a *relational* break: correctly-styled controls that render
// detached from the thing they belong to. The decision-queue redesign shipped
// exactly that — Approve/Deny pills floating on the page background because their
// container painted no surface — with every test in this file green. The
// surface-containment test below closes that class.

import { test, expect, type Locator } from "@playwright/test";
import { mockFetch, COCKPIT_FIXTURE } from "./fixtures";

// ---- design tokens (must match styles.css :root) -------------------------

const ACCENT = "rgb(135, 175, 215)"; // --accent (CLI xterm 110)
const WARN = "rgb(215, 175, 95)"; // --warn (CLI xterm 179)
const DANGER = "rgb(215, 135, 135)"; // --danger (CLI xterm 174)
const TRANSPARENT = "rgba(0, 0, 0, 0)";

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

  // Inbox row: same compact task-row shape as the calm list — opaque bg, no
  // left stripe; status label carries the tone color instead.
  const inboxRow = page.locator(".task-row.is-inbox").first();
  const rowStyle = await inboxRow.evaluate((el) => {
    const s = getComputedStyle(el);
    return {
      bg: s.backgroundColor,
      leftWidth: s.borderLeftWidth,
    };
  });
  expect(rowStyle.bg).not.toBe(TRANSPARENT);
  expect(rowStyle.leftWidth).not.toBe("3px");

  // Status label paints with the tone color (waiting -> warn), not default ink.
  const status = page.locator(".task-row-status").first();
  expect(await status.evaluate((el) => getComputedStyle(el).color)).toBe(WARN);

  // Task rows have the compact list padding (would be 0 if unstyled).
  const row = page.locator(".task-row").first();
  expect(await row.evaluate((el) => getComputedStyle(el).paddingTop)).toBe("10px");

  // Single new-task entry: bottom-nav only (no in-list dashed CTA).
  await expect(page.locator(".new-task-row")).toHaveCount(0);
  await expect(newButton).toBeVisible();
});

// A control group must read as part of the thing it acts on. We assert it by
// walking up to the nearest ancestor that actually paints (background or border).
// For a correctly-parented group that surface is its card, a few levels up and
// inside the route outlet. For an orphaned group the walk escapes the outlet
// entirely and only stops at the app root's page paper — which is precisely the
// "floating on the background" look, and the exact shape of the shipped bug.

// Two inbox entries on purpose: the lead entry and the ones behind it are styled
// by different rules, and the shipped regression only affected the latter.
const TWO_ATTENTION_ITEMS = {
  ...COCKPIT_FIXTURE,
  cards: [
    ...COCKPIT_FIXTURE.cards,
    {
      id: "api/migrate-db",
      qualified_handle: "api/migrate-db",
      repo: "api",
      title: "Migrate database schema",
      status: "error",
      status_explanation: "Worktree is missing",
      actions: [
        { action: "repair", label: "Repair", destructive: false, confirmation_required: false },
      ],
    },
  ],
  inbox: {
    items: [
      { task_handle: "api/migrate-db", severity: 1 },
      { task_handle: "web/fix-login", severity: 2 },
    ],
  },
};

test("dashboard action groups sit on a card, not on the page background", async ({
  page,
}) => {
  await mockFetch(page, { "/api/cockpit": TWO_ATTENTION_ITEMS });
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  const groups = page.locator('[data-testid="outlet-dashboard"] .action-row');
  expect(await groups.count()).toBeGreaterThan(0);

  const findings = await groups.evaluateAll((nodes) =>
    nodes.map((node) => {
      const transparent = "rgba(0, 0, 0, 0)";
      const outlet = document.querySelector('[data-testid="outlet-dashboard"]');
      let surface = node.parentElement;
      while (surface) {
        const style = getComputedStyle(surface);
        const paints =
          style.backgroundColor !== transparent ||
          parseFloat(style.borderTopWidth) > 0 ||
          parseFloat(style.borderBottomWidth) > 0;
        if (paints) break;
        surface = surface.parentElement;
      }
      const box = node.getBoundingClientRect();
      const surfaceBox = surface?.getBoundingClientRect();
      return {
        action: node.querySelector("button")?.textContent ?? "?",
        surface: surface ? `${surface.tagName}.${surface.className}` : "NONE",
        // A card lives inside the route outlet. Page paper does not.
        onCard: surface != null && outlet != null && outlet.contains(surface),
        // Does that surface actually wrap the controls, or just sit behind them?
        contained:
          surfaceBox != null &&
          box.left >= surfaceBox.left - 1 &&
          box.right <= surfaceBox.right + 1 &&
          box.top >= surfaceBox.top - 1 &&
          box.bottom <= surfaceBox.bottom + 1,
      };
    }),
  );

  for (const finding of findings) {
    expect(
      finding.onCard,
      `action group "${finding.action}" paints no card of its own — the nearest surface is ` +
        `${finding.surface}, outside the route outlet, so the controls float on the page`,
    ).toBe(true);
    expect(
      finding.contained,
      `action group "${finding.action}" overflows its surface (${finding.surface})`,
    ).toBe(true);
  }
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
