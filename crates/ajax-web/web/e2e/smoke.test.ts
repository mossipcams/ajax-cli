import { test, expect } from "@playwright/test";
import { COCKPIT_FIXTURE, DETAIL_FIXTURE, mockFetch, sessionSnapshotJson } from "./fixtures";



test("dashboard renders tasks from cockpit fixture", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html");

  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText("api/add-auth")).toBeVisible();
});

test("operator action and reload project authoritative task state", async ({ page }) => {
  const runningCockpit = {
    ...COCKPIT_FIXTURE,
    cards: COCKPIT_FIXTURE.cards.map((card) =>
      card.qualified_handle === "web/fix-login"
        ? { ...card, status: "running", status_explanation: "Agent working", actions: [] }
        : card,
    ),
    inbox: { items: [] },
  };
  const runningDetail = {
    ...DETAIL_FIXTURE,
    status: "running",
    status_explanation: "Agent working",
    actions: [],
  };
  await mockFetch(page, {
    __cockpit_after_review__: runningCockpit,
    __detail_after_review__: runningDetail,
  });
  await page.goto("/app.html");

  await page.getByText("web/fix-login", { exact: true }).first().click();
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText("Waiting for review")).toBeVisible();
  await page.getByRole("button", { name: "Review" }).click();

  await expect(page.getByText("Agent working")).toBeVisible();
  await page.reload();
  await expect(page.getByText("Agent working")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByRole("button", { name: "Review" })).toHaveCount(0);
});

test("project filter shows only matching repo tasks", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/p/web");

  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText("api/add-auth")).not.toBeVisible();
});

test("task detail renders server status and actions", async ({ page }, testInfo) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");

  if (testInfo.project.name === "mobile-webkit") {
    await expect(page.locator(".interact-pill")).toContainText("Waiting", { timeout: 10_000 });
  } else {
    await expect(page.getByText("Waiting for review")).toBeVisible({ timeout: 10_000 });
  }
  await expect(page.locator("[data-action='review']")).toBeVisible();
});

test("non-destructive action completes without a second tap", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-action='review']")).toBeVisible({ timeout: 10_000 });

  await page.locator("[data-action='review']").click();

  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 5_000 });
});

test("destructive action requires two taps to execute", async ({ page }) => {
  await mockFetch(page);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.locator("[data-action='drop']")).toBeVisible({ timeout: 10_000 });

  await page.locator("[data-action='drop']").click();
  const confirmPanel = page.getByTestId("result-panel-confirm");
  await expect(confirmPanel).toBeVisible({ timeout: 3_000 });

  await confirmPanel.getByRole("button", { name: "Confirm" }).click();
  await expect(page.locator("[data-outlet='task']")).toBeVisible({ timeout: 5_000 });
});

test("restart agent confirm from task details reaches the operation endpoint", async ({ page }) => {
  await mockFetch(page, {
    __detail__: {
      ...DETAIL_FIXTURE,
      agent: "Cursor",
      session_capable: true,
      actions: [
        { action: "restart-agent", label: "Restart agent", destructive: false, confirmation_required: true },
      ],
    },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
    const mockedFetch = globalThis.fetch;
    globalThis.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      if (new URL(url, location.href).pathname === "/api/operations" && init?.method === "POST") {
        sessionStorage.setItem("ajax-e2e-operation-body", String(init.body));
      }
      return mockedFetch(input, init);
    };
  });
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.send(sessionSnapshotJson({ reset: true, cursor: 0, transcript: [] }));
  });
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-details")).toBeVisible({ timeout: 10_000 });

  await page.getByTestId("session-details").click();
  await page.getByTestId("session-restart-agent").click();
  await page.getByTestId("result-panel-confirm").getByRole("button", { name: "Confirm" }).click();

  await expect
    .poll(() => page.evaluate(() => sessionStorage.getItem("ajax-e2e-operation-body")))
    .toContain('"restart-agent"');
});
