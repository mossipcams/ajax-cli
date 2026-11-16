import { expect, test, type Page } from "@playwright/test";
import {
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
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    server = socket;
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

  return (event: SessionServerEvent) => {
    server?.send(sessionEventJson(nextCursor++, event));
  };
}

test("#1636 keeps the reading position when the chat head grows and shrinks", async ({
  page,
}) => {
  const send = await openSessionChat(page);

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
