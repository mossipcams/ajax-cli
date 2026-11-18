import { expect, test, type Page } from "@playwright/test";
import { DETAIL_FIXTURE, mockFetch, sessionEventJson, sessionSnapshotJson } from "./fixtures";

const CHAT_DETAIL = { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true };

async function openChatWithEvents(page: Page, events: Record<string, unknown>[]) {
  await mockFetch(page, { __detail__: CHAT_DETAIL });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.send(sessionSnapshotJson({ cursor: 0, model: "auto", turnState: "idle" }));
    events.forEach((event, index) => socket.send(sessionEventJson(index, event)));
  });
  await page.goto("/app.html#/session/web%2Ffix-login");
}

async function viewportCheck(
  page: Page,
  locator: ReturnType<Page["getByRole"]>,
): Promise<void> {
  const bottom = await locator.evaluate((node) => node.getBoundingClientRect().bottom);
  const innerHeight = await page.evaluate(() => window.innerHeight);
  expect(bottom).toBeLessThanOrEqual(innerHeight);
}

async function scrollState(
  locator: ReturnType<Page["locator"]>,
): Promise<{ scrollable: boolean; overflowY: string }> {
  return locator.evaluate((node) => ({
    scrollable: node.scrollHeight > node.clientHeight,
    overflowY: getComputedStyle(node).overflowY,
  }));
}

test("#1665 keeps Approve/Reject in the viewport when the permission detail overflows", async ({
  page,
}) => {
  const detail = Array.from(
    { length: 40 },
    (_, index) => `Question ${index + 1}: which of the four deployment targets should the agent use for step ${index + 1}, and why does it differ from the default route?`,
  ).join("\n");
  await openChatWithEvents(page, [
    { type: "permission_request", requestId: "p1", title: "Answer these questions", detail },
  ]);

  const decision = page.getByTestId("session-decision");
  await expect(decision).toBeVisible();

  const body = page.locator(".session-decision-detail");
  const state = await scrollState(body);
  expect(state.overflowY).toBe("auto");
  expect(state.scrollable).toBe(true);

  await viewportCheck(page, page.getByRole("button", { name: "Approve" }));
  await viewportCheck(page, page.getByRole("button", { name: "Reject" }));
});

test("#1665 keeps a short permission detail unscrolled and its actions reachable", async ({
  page,
}) => {
  await openChatWithEvents(page, [
    { type: "permission_request", requestId: "p2", title: "Run tests?", detail: "cargo test" },
  ]);

  const body = page.locator(".session-decision-detail");
  await expect(page.getByTestId("session-decision")).toBeVisible();
  const state = await scrollState(body);
  expect(state.overflowY).toBe("auto");
  expect(state.scrollable).toBe(false);

  await viewportCheck(page, page.getByRole("button", { name: "Approve" }));
});

test("#1665 keeps Accept/Decline/Cancel in the viewport when elicitation fields overflow", async ({
  page,
}) => {
  const properties: Record<string, unknown> = {};
  for (let i = 0; i < 20; i += 1) {
    properties[`field-${i}`] = { type: "string", title: `Question ${i + 1} about the rollout plan` };
  }
  await openChatWithEvents(page, [
    {
      type: "elicitation_request",
      requestId: "e1",
      message: "Answer these questions about the rollout plan",
      schema: { properties, required: [] },
    },
  ]);

  const panel = page.getByTestId("session-elicitation");
  await expect(panel).toBeVisible();

  const fields = page.locator(".session-elicitation-fields");
  const state = await scrollState(fields);
  expect(state.overflowY).toBe("auto");
  expect(state.scrollable).toBe(true);

  await viewportCheck(page, page.getByRole("button", { name: "Accept" }));
  await viewportCheck(page, page.getByRole("button", { name: "Decline" }));
  await viewportCheck(page, page.getByRole("button", { name: "Cancel" }));
});
