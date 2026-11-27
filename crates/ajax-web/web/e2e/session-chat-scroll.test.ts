import { expect, test, type Page } from "@playwright/test";
import {
  COCKPIT_FIXTURE,
  DETAIL_FIXTURE,
  emulateCoarsePointer,
  mockFetch,
  sessionEventJson,
  sessionSnapshotJson,
  type SessionServerEvent,
} from "./fixtures";

type TranscriptEntry = { cursor: number; event: SessionServerEvent };

async function openSessionChat(page: Page) {
  await emulateCoarsePointer(page);
  await mockFetch(page, {
    "/api/cockpit": {
      ...COCKPIT_FIXTURE,
      cards: COCKPIT_FIXTURE.cards.map((card) =>
        card.qualified_handle === "web/fix-login" ? { ...card, session_capable: true } : card,
      ),
    },
    __detail__: {
      ...DETAIL_FIXTURE,
      session_capable: true,
      agent: "Cursor",
      status: "running",
      status_explanation: "Agent working",
    },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });

  const transcript: TranscriptEntry[] = [];
  for (let i = 0; i < 30; i += 1) {
    transcript.push({
      cursor: transcript.length,
      event: { type: "message", role: "user", text: `Earlier question ${i}`, itemId: `u${i}` },
    });
    transcript.push({
      cursor: transcript.length,
      event: {
        type: "message",
        role: "agent",
        text: `Earlier answer ${i}. ${"Padding to overflow the transcript. ".repeat(4)}`,
        itemId: `a${i}`,
      },
    });
  }
  transcript.push({
    cursor: transcript.length,
    event: {
      type: "tool_call",
      callId: "call-0",
      title: "Read config",
      kind: "read",
      status: "completed",
      locations: [],
    },
  });

  let nextCursor = transcript.length + 1;
  let server: { send: (data: string) => void } | undefined;
  let lastClientMessageId: string | undefined;
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    server = socket;
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as { clientMessageId?: string };
      if (event.clientMessageId) lastClientMessageId = event.clientMessageId;
    });
    socket.send(
      sessionSnapshotJson({
        cursor: transcript.length,
        model: "auto",
        turnState: "idle",
        reset: true,
        transcript: transcript.map(({ cursor, event }) =>
          JSON.parse(sessionEventJson(cursor, event)) as Record<string, unknown>,
        ),
      }),
    );
  });
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible();
  await expect(page.getByTestId("session-message-agent").last()).toContainText("Earlier answer 29");

  return {
    send: (event: SessionServerEvent) => {
      server?.send(sessionEventJson(nextCursor++, event));
    },
    clientMessageId: () => lastClientMessageId,
  };
}

test("#1636 keeps the reading position when the chat head grows and shrinks", async ({
  page,
}) => {
  const { send } = await openSessionChat(page);

  const thread = page.getByTestId("session-thread");
  await thread.evaluate((node) => {
    node.scrollTop = -500;
  });
  await page.waitForTimeout(50);
  expect(await thread.evaluate((node) => node.scrollTop)).toBeLessThanOrEqual(-400);

  const anchor = page.getByTestId("session-message-agent").filter({ hasText: "Earlier answer 12." });
  const readPosition = async () => {
    await page.evaluate(
      () => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))),
    );
    return {
      top: await anchor.evaluate((node) => node.getBoundingClientRect().top),
      threadTop: await thread.evaluate((node) => node.getBoundingClientRect().top),
    };
  };

  const start = await readPosition();

  send({ type: "message", role: "user", text: "Check the config", itemId: "u-live" });
  await expect(page.getByTestId("session-head-idle")).toBeVisible();
  const grown = await readPosition();
  expect(grown.threadTop - start.threadTop).toBeGreaterThanOrEqual(20);
  expect(Math.abs(grown.top - start.top)).toBeLessThanOrEqual(2);

  send({
    type: "tool_call",
    callId: "call-1",
    title: "Read config",
    kind: "read",
    status: "in_progress",
    locations: [],
  });
  await expect(page.getByTestId("session-head-idle")).toHaveCount(0);
  const shrunk = await readPosition();
  expect(Math.abs(shrunk.top - start.top)).toBeLessThanOrEqual(2);
});

test("#1629 Chat → Diff → Back restores the reading position", async ({ page }) => {
  await openSessionChat(page);

  const thread = page.getByTestId("session-thread");
  await thread.evaluate((node) => {
    node.scrollTop = -500;
  });
  await expect.poll(() => thread.evaluate((node) => -node.scrollTop)).toBeGreaterThanOrEqual(400);
  const before = await thread.evaluate((node) => -node.scrollTop);

  await page.getByTestId("session-details").click();
  await page.getByRole("button", { name: "Show diff" }).click();
  await expect(page.getByTestId("diff-review")).toBeVisible();
  await page.getByTestId("diff-review").getByRole("button", { name: "← Back" }).click();

  await expect(page.getByTestId("session-chat")).toBeVisible();
  await expect
    .poll(async () =>
      Math.abs((await page.getByTestId("session-thread").evaluate((node) => -node.scrollTop)) - before),
    )
    .toBeLessThanOrEqual(4);
});

test("#1612 sent message bubble keeps its height when the server accepts it", async ({ page }) => {
  const { send, clientMessageId } = await openSessionChat(page);

  await page.getByLabel("Message").fill("alpha bravo charlie delta echo foxtrot golf");
  await page.getByLabel("Message").press("Enter");

  const bubble = page.getByTestId("session-message-user").last();
  await expect(bubble).toContainText("Sending…");
  await expect.poll(clientMessageId).toBeDefined();
  const pending = await bubble.boundingBox();

  send({ type: "prompt_accepted", clientMessageId: clientMessageId()! });
  await expect(bubble.getByText("Sending…")).toHaveCount(0);
  const accepted = await bubble.boundingBox();

  expect(pending).not.toBeNull();
  expect(accepted).not.toBeNull();
  expect(Math.abs(accepted!.height - pending!.height)).toBeLessThanOrEqual(1);
});
