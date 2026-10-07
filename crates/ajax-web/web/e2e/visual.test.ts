import { test, expect, type Locator } from "@playwright/test";
import {
  mockFetch,
  COCKPIT_FIXTURE,
  DETAIL_FIXTURE,
  sessionEventJson,
  sessionSnapshotJson,
  type SessionServerEvent,
} from "./fixtures";


const ACCENT = "rgb(135, 175, 215)";
const WARN = "rgb(215, 175, 95)";
const DANGER = "rgb(215, 135, 135)";
const TRANSPARENT = "rgba(0, 0, 0, 0)";

function bg(locator: Locator) {
  return locator.evaluate((el) => getComputedStyle(el).backgroundColor);
}


test("dashboard chrome and cards carry the cockpit stylesheet", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });

  const newButton = page.locator('.bottom-nav button[data-bottom-action="new-task"]');
  expect(await bg(newButton)).toBe(ACCENT);

  const activePill = page.locator(".project-pill.is-active").first();
  expect(await bg(activePill)).toBe(ACCENT);

  const taskRow = page.locator('.task-row[data-handle="web/fix-login"]').first();
  const rowStyle = await taskRow.evaluate((el) => {
    const s = getComputedStyle(el);
    return {
      bg: s.backgroundColor,
      leftWidth: s.borderLeftWidth,
    };
  });
  expect(rowStyle.bg).not.toBe(TRANSPARENT);
  expect(rowStyle.leftWidth).not.toBe("3px");

  const status = taskRow.locator(".task-row-status");
  expect(await status.evaluate((el) => getComputedStyle(el).color)).toBe(WARN);

  const row = page.locator(".task-row").first();
  expect(await row.evaluate((el) => getComputedStyle(el).paddingTop)).toBe("10px");

  await expect(page.locator(".new-task-row")).toHaveCount(0);
  await expect(newButton).toBeVisible();
});


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
        onCard: surface != null && outlet != null && outlet.contains(surface),
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

  const primary = page.locator(".action.primary").first();
  expect(await bg(primary)).toBe(ACCENT);

  const destructive = page.locator('.action[data-destructive="true"]').first();
  expect(await destructive.evaluate((el) => getComputedStyle(el).color)).toBe(DANGER);

  const panel = page.locator(".interact-panel").first();
  const panelStyle = await panel.evaluate((el) => {
    const s = getComputedStyle(el);
    return { bg: s.backgroundColor, borderTopWidth: s.borderTopWidth };
  });
  expect(panelStyle.bg).toBe(TRANSPARENT);
  expect(panelStyle.borderTopWidth).toBe("1px");

  const pill = page.locator(".interact-pill").first();
  expect(await pill.evaluate((el) => getComputedStyle(el).color)).toBe(WARN);
  expect(await pill.evaluate((el) => getComputedStyle(el, "::before").content)).toBe('"◦"');

  const title = page.locator(".detail-title");
  expect(await title.evaluate((el) => getComputedStyle(el).fontSize)).toBe("16px");
});

test("settings view sections are styled", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/settings");
  await expect(page.locator("[data-testid='outlet-settings']")).toBeVisible({ timeout: 10_000 });

  const section = page.locator(".settings-section").first();
  const style = await section.evaluate((el) => {
    const s = getComputedStyle(el);
    return { borderTopWidth: s.borderTopWidth, paddingTop: s.paddingTop };
  });
  expect(style.borderTopWidth).toBe("1px");
  expect(style.paddingTop).toBe("16px");
});

test("session chat failure surface and single disclosure indent", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });

  let nextCursor = 0;
  const send = (socket: { send: (data: string) => void }, event: SessionServerEvent) => {
    socket.send(sessionEventJson(nextCursor++, event));
  };

  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.send(sessionSnapshotJson({ cursor: nextCursor, model: "auto", turnState: "idle" }));
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as { type?: string };
      if (event.type !== "prompt") return;
      send(socket, { type: "message", role: "user", text: "Run tests" });
      send(socket, {
        type: "tool_call",
        callId: "fail-1",
        title: "cargo test",
        kind: "execute",
        status: "failed",
        content: [{ type: "text", text: "assertion `left == right` failed" }],
      });
      send(socket, { type: "message", role: "agent", text: "Tests failed." });
      send(socket, { type: "turn_end", stopReason: "end_turn" });
    });
  });

  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });

  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible({ timeout: 10_000 });
  await page.getByLabel("Message").fill("Run tests");
  await page.getByLabel("Message").press("Enter");

  await expect(page.getByTestId("session-tool-output")).toContainText("assertion");

  const layout = await page.evaluate(() => {
    const turnWork = document.querySelector('[data-testid="session-turn-work"]') as HTMLElement;
    const failureBody = document.querySelector(".session-toolcard-body.is-failure") as HTMLElement;
    const output = document.querySelector('[data-testid="session-tool-output"]') as HTMLElement;
    const failureStyle = failureBody ? getComputedStyle(failureBody) : null;
    const turnBox = turnWork.getBoundingClientRect();
    const outputBox = output.getBoundingClientRect();
    return {
      failureBorderLeft: failureStyle?.borderLeftWidth ?? "0px",
      failureBg: failureStyle?.backgroundColor ?? "",
      outputLeft: outputBox.left,
      turnLeft: turnBox.left,
      indentPx: outputBox.left - turnBox.left,
    };
  });

  expect(layout.failureBorderLeft).not.toBe("0px");
  expect(layout.failureBg).not.toBe(TRANSPARENT);
  expect(layout.indentPx).toBeGreaterThan(20);
  expect(layout.indentPx).toBeLessThan(44);

  await expect(page.getByTestId("session-head-status")).toHaveCount(0);
});

test("session chat text block kinds paint distinct surfaces", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });

  let nextCursor = 0;
  const send = (socket: { send: (data: string) => void }, event: SessionServerEvent) => {
    socket.send(sessionEventJson(nextCursor++, event));
  };

  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.send(sessionSnapshotJson({ cursor: nextCursor, model: "auto", turnState: "idle" }));
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as { type?: string };
      if (event.type !== "prompt") return;
      send(socket, { type: "message", role: "user", text: "Inspect files" });
      send(socket, {
        type: "tool_call",
        callId: "search-1",
        title: "search",
        kind: "search",
        status: "failed",
        content: [{ type: "text", text: "src/main.rs\nsrc/lib.rs" }],
      });
      send(socket, {
        type: "tool_call",
        callId: "read-1",
        title: "read",
        kind: "read",
        status: "failed",
        content: [{ type: "text", text: "fn main() {}" }],
      });
      send(socket, {
        type: "tool_call",
        callId: "run-1",
        title: "cargo test",
        kind: "execute",
        status: "failed",
        content: [{ type: "text", text: "test result: FAILED" }],
      });
      send(socket, { type: "message", role: "agent", text: "Done." });
      send(socket, { type: "turn_end", stopReason: "end_turn" });
    });
  });

  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });

  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible({ timeout: 10_000 });
  await page.getByLabel("Message").fill("Inspect files");
  await page.getByLabel("Message").press("Enter");

  await expect(page.locator('[data-block-kind="search"]')).toBeVisible();

  const surfaces = await page.evaluate(() => {
    const paint = (el: Element) => {
      const s = getComputedStyle(el);
      return {
        borderStyle: s.borderStyle,
        borderTopWidth: s.borderTopWidth,
        borderLeftWidth: s.borderLeftWidth,
        backgroundColor: s.backgroundColor,
      };
    };
    const search = document.querySelector('[data-block-kind="search"]');
    const read = document.querySelector('[data-block-kind="read"]');
    const output = document.querySelector('[data-block-kind="output"]');
    if (!search || !read || !output) return null;
    return {
      search: paint(search),
      read: paint(read),
      output: paint(output),
    };
  });

  expect(surfaces).not.toBeNull();
  const { search, read, output } = surfaces!;

  const signature = (s: typeof search) =>
    `${s.borderStyle}|${s.borderTopWidth}|${s.borderLeftWidth}|${s.backgroundColor}`;

  expect(new Set([signature(search), signature(read), signature(output)]).size).toBe(3);
});
