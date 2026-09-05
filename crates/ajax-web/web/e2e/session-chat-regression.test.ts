import { expect, test, type Page } from "@playwright/test";
import {
  COCKPIT_FIXTURE,
  DETAIL_FIXTURE,
  mockFetch,
  sessionEventJson,
  sessionResumeCursor,
  sessionSnapshotJson,
  type SessionServerEvent,
} from "./fixtures";

const HARNESSES = ["Cursor", "Codex", "Claude", "Pi"] as const;

type TranscriptEntry = { cursor: number; event: SessionServerEvent };

async function mockScriptedSession(
  page: Page,
  harness: (typeof HARNESSES)[number],
  options: { errorTurn?: boolean } = {},
) {
  const transcript: TranscriptEntry[] = [];
  let nextCursor = 0;
  let turn = 0;

  function record(event: SessionServerEvent): TranscriptEntry {
    const entry = { cursor: nextCursor++, event };
    transcript.push(entry);
    return entry;
  }

  function send(socket: { send: (data: string) => void }, entry: TranscriptEntry) {
    socket.send(sessionEventJson(entry.cursor, entry.event));
  }

  function open(socket: { send: (data: string) => void; url: () => string }) {
    const resumeFrom = sessionResumeCursor(socket.url());
    socket.send(sessionSnapshotJson({ cursor: nextCursor, model: "auto", turnState: "idle" }));
    for (const entry of transcript) {
      if (entry.cursor >= resumeFrom) send(socket, entry);
    }
  }

  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    open(socket);
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as {
        type?: string;
        text?: string;
        clientMessageId?: string;
      };
      if (event.type !== "prompt" || !event.text) return;
      if (event.clientMessageId) {
        send(
          socket,
          record({ type: "prompt_accepted", clientMessageId: event.clientMessageId }),
        );
      }
      turn += 1;
      record({ type: "message", role: "user", text: event.text });
      if (options.errorTurn) {
        send(socket, record({ type: "turn_end", stopReason: "error" }));
        return;
      }
      const reply =
        turn === 1 ? `${harness}-ONE`
        : turn === 2 ? `### ${harness}-TWO\n\n- **bold** and \`code\`\n- second bullet\n\n1. first\n2. second\n\n\`\`\`txt\n${harness}-CODE\n\`\`\``
        : `${harness}-THREE: ${harness}-TWO`;
      send(socket, record({ type: "message", role: "agent", text: reply }));
      send(socket, record({ type: "turn_end", stopReason: "end_turn" }));
    });
  });
}

for (const harness of HARNESSES) {
  test(`${harness} creates a chat task and completes three turns`, async ({ page }) => {
    const slug = `smoke-${harness.toLowerCase()}`;
    const handle = `web/${slug}`;
    const card = {
      ...COCKPIT_FIXTURE.cards[0],
      id: handle,
      qualified_handle: handle,
      title: slug,
      agent: harness.toLowerCase(),
      session_capable: true,
    };
    await mockFetch(page, {
      "/api/tasks": { ok: true, cockpit: { ...COCKPIT_FIXTURE, cards: [card] } },
      __detail__: {
        ...DETAIL_FIXTURE,
        qualified_handle: handle,
        title: slug,
        agent: harness,
        session_capable: true,
      },
    });
    await page.addInitScript(() => {
      localStorage.setItem("ajax.web.session.orchestrationChat", "true");
    });
    await mockScriptedSession(page, harness);

    await page.goto("/app.html");
    await page.getByRole("button", { name: "New", exact: true }).click();
    const sheet = page.getByTestId("new-task-sheet");
    await sheet.getByLabel("Title").fill(slug);
    await sheet.getByRole("radio", { name: harness, exact: true }).click();
    await sheet.getByRole("button", { name: "Next", exact: true }).click();
    await expect(sheet.getByTestId("new-task-model-page")).toBeVisible();
    await sheet.getByRole("button", { name: "Start", exact: true }).click();

    await expect(page).toHaveURL(new RegExp(`#/session/${encodeURIComponent(handle)}$`));
    await expect(page.getByTestId("session-chat")).toBeVisible();

    const sendPrompt = async (prompt: string, reply: string) => {
      await page.getByLabel("Message").fill(prompt);
      await page.getByLabel("Message").press("Enter");
      await expect(page.getByTestId("session-message-agent").last()).toContainText(reply);
    };
    await sendPrompt("First response", `${harness}-ONE`);
    await sendPrompt("Render the formatting sample", `${harness}-TWO`);

    const formatted = page.getByTestId("session-message-agent").last();
    await expect(formatted.locator("h3")).toHaveText(`${harness}-TWO`);
    await expect(formatted.locator("strong")).toHaveText("bold");
    await expect(formatted.locator("code:not(pre code)")).toHaveText("code");
    await expect(formatted.locator("pre code")).toHaveText(`${harness}-CODE`);
    await expect(formatted.locator("ul li")).toHaveCount(2);
    await expect(formatted.locator("ol li")).toHaveCount(2);
    expect(await formatted.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true);

    await sendPrompt("Recall the previous heading", `${harness}-THREE: ${harness}-TWO`);
    await expect(page.getByTestId("session-message-user")).toHaveCount(3);
    await expect(page.getByTestId("session-message-agent")).toHaveCount(3);

    await page.goto("/app.html");
    await page.goto(`/app.html#/session/${encodeURIComponent(handle)}`);
    await expect(page.getByTestId("session-message-user")).toHaveCount(3);
    await expect(page.getByTestId("session-message-agent")).toHaveCount(3);

    await page.reload();
    await expect(page.getByTestId("session-message-user")).toHaveCount(3);
    await expect(page.getByTestId("session-message-agent")).toHaveCount(3);
    // #888: disposing the pre-open socket during reload must not append a false failure.
    await expect(page.getByTestId("session-note-error")).toHaveCount(0);
  });
}

/** A session with enough replayed history to overflow the thread, and a turn
 * that never answers on its own — so the follow-up queue and the cancel
 * handshake can be driven from the test. */
async function mockHeldTurn(page: Page) {
  let nextCursor = 0;

  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    const send = (event: SessionServerEvent) =>
      socket.send(sessionEventJson(nextCursor++, event));

    socket.send(sessionSnapshotJson({ cursor: nextCursor, model: "auto", turnState: "idle" }));
    for (let i = 0; i < 12; i += 1) {
      send({ type: "message", role: "user", text: `Earlier question ${i}`, itemId: `u${i}` });
      send({
        type: "message",
        role: "agent",
        text: `Earlier answer ${i}. ${"Padding to make this history overflow the band. ".repeat(4)}`,
        itemId: `a${i}`,
      });
    }
    send({ type: "turn_end", stopReason: "end_turn" });

    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as {
        type?: string;
        text?: string;
        clientMessageId?: string;
      };
      if (event.type === "prompt" && event.text) {
        if (event.clientMessageId) {
          send({ type: "prompt_accepted", clientMessageId: event.clientMessageId });
        }
        send({
          type: "message",
          role: "user",
          text: event.text,
          itemId: event.clientMessageId ? `u:${event.clientMessageId}` : `u${nextCursor}`,
        });
        return;
      }
      // The prompt is accepted and then held: no agent output, no turn_end,
      // exactly like a long turn. Cancel is what ends it.
      if (event.type === "cancel") send({ type: "turn_end", stopReason: "cancelled" });
    });
  });
}

test("opens at the latest content and holds a follow-up until the turn resolves", async ({
  page,
}) => {
  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
    // Sample the thread's position on every frame from before it mounts: an
    // overflowing conversation must never be observed sitting away from its
    // latest content while the operator has not scrolled.
    const samples: { top: number; overflow: number }[] = [];
    (window as unknown as { __threadSamples: typeof samples }).__threadSamples = samples;
    const tick = () => {
      const thread = document.querySelector('[data-testid="session-thread"]');
      if (thread) {
        samples.push({
          top: thread.scrollTop,
          overflow: thread.scrollHeight - thread.clientHeight,
        });
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
  await mockHeldTurn(page);

  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible();
  await expect(page.getByTestId("session-message-agent").last()).toContainText("Earlier answer 11");

  const strandedFrames = await page.evaluate(() => {
    const samples = (window as unknown as { __threadSamples: { top: number; overflow: number }[] })
      .__threadSamples;
    return {
      overflowed: samples.filter((sample) => sample.overflow > 48).length,
      stranded: samples.filter((sample) => sample.overflow > 48 && sample.top < sample.overflow - 48)
        .length,
    };
  });
  expect(strandedFrames.overflowed).toBeGreaterThan(0);
  expect(strandedFrames.stranded).toBe(0);

  await page.getByLabel("Message").fill("Start the work");
  await page.getByLabel("Message").press("Enter");
  await expect(page.getByTestId("session-head")).toContainText("Working");

  // First Enter while busy queues one editable follow-up.
  await page.getByLabel("Message").fill("And then deploy");
  await page.getByLabel("Message").press("Enter");
  await expect(page.getByTestId("session-queued")).toContainText("Queued");
  await expect(page.getByTestId("session-queued")).toContainText("And then deploy");
  await expect(page.getByRole("button", { name: "Stop & send" })).toBeVisible();

  // Second Enter cancels the live turn, and the follow-up waits for it to
  // resolve rather than racing it.
  await page.getByLabel("Message").press("Enter");
  await expect(page.getByTestId("session-note-info")).toContainText("Stopped");
  await expect(page.getByTestId("session-queued")).toHaveCount(0);
  await expect(page.getByTestId("session-message-user").last()).toContainText("And then deploy");
});

/** One turn carrying everything ACP separates: reasoning, a plan, a tool call
 * with a diff, and a tool call with command output. The long diff line is the
 * point of the width assertions — `white-space: pre` inside a flex column is
 * exactly how a code block starts panning the whole phone surface sideways. */
async function mockTypedTurn(page: Page) {
  const LONG = "a".repeat(400);
  let nextCursor = 0;

  function send(socket: { send: (data: string) => void }, event: SessionServerEvent) {
    socket.send(sessionEventJson(nextCursor++, event));
  }

  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.send(sessionSnapshotJson({ cursor: nextCursor, model: "auto", turnState: "idle" }));
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as { type?: string; text?: string };
      if (event.type !== "prompt") return;
      const events: SessionServerEvent[] = [
        { type: "message", role: "thought", text: "Deciding where the port is set" },
        { type: "status", state: "running", detail: "Indexing workspace" },
        {
          type: "plan",
          entries: [
            { content: "Find the port", status: "completed" },
            { content: "Change it", status: "in_progress" },
          ],
        },
        {
          type: "tool_call",
          callId: "call-1",
          title: "Edit config",
          kind: "edit",
          status: "in_progress",
          locations: ["/repo/crates/ajax-web/src/config.ts"],
        },
        {
          type: "tool_call",
          callId: "call-1",
          title: "Edit config",
          kind: "edit",
          status: "completed",
          locations: [],
          content: [
            {
              type: "diff",
              path: "/repo/crates/ajax-web/src/config.ts",
              oldText: `const port = 1;\n// ${LONG}\n`,
              newText: `const port = 2;\n// ${LONG}\n`,
            },
          ],
        },
        {
          type: "tool_call",
          callId: "call-2",
          title: "cargo test",
          kind: "execute",
          status: "failed",
          content: [{ type: "text", text: `error: assertion failed ${LONG}` }],
        },
        { type: "usage", used: 92, size: 100 },
        {
          type: "message",
          role: "agent",
          // Long enough that the thread overflows its band. The flex-shrink bug
          // this guards only appears once the column has to give up height.
          text: `Changed the port to 2.\n\n${"The config module reads it once at startup and hands it to the listener. ".repeat(
            12,
          )}`,
        },
        { type: "turn_end", stopReason: "end_turn" },
      ];
      for (const next of events) send(socket, next);
    });
  });
}

test("a turn keeps its tool calls, diff, plan and reasoning one tap away", async ({ page }) => {
  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });
  await mockTypedTurn(page);

  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible();
  await page.getByLabel("Message").fill("Change the port");
  await page.getByLabel("Message").press("Enter");

  await expect(page.getByTestId("session-message-agent")).toContainText("Changed the port to 2.");

  await expect(page.getByTestId("session-head-status")).toHaveCount(0);

  // The turn carries one disclosure, and a failure inside it opens the timeline
  // without being asked. Two calls, merged by id — the update revised the edit
  // rather than adding a row.
  await expect(page.getByTestId("session-turn-work-summary")).toHaveCount(1);
  await expect(page.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "true");
  await expect(page.getByTestId("session-tool-card")).toHaveCount(2);
  const edit = page.getByTestId("session-tool-card").first();
  await expect(edit).toHaveAttribute("data-status", "completed");

  // The failure opens itself; the success stays quiet until asked.
  await expect(page.getByTestId("session-tool-output")).toContainText("assertion failed");
  await expect(page.getByTestId("session-tool-diff")).toHaveCount(0);
  await edit.getByRole("button").click();
  await expect(page.getByTestId("session-tool-diff")).toContainText("-const port = 1;");
  await expect(page.getByTestId("session-tool-diff")).toContainText("+const port = 2;");

  await expect(page.getByTestId("session-plan").getByRole("listitem")).toHaveCount(2);

  await expect(page.getByTestId("session-thinking-body")).toHaveCount(0);
  await page.getByTestId("session-thinking").getByRole("button").click();
  await expect(page.getByTestId("session-thinking-body")).toContainText("Deciding where the port");

  // A 400-character diff line must scroll inside its own block, never widen the
  // surface: a phone that pans sideways loses the composer off-screen.
  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
    return {
      page: doc.scrollWidth - doc.clientWidth,
      thread: thread.scrollWidth - thread.clientWidth,
    };
  });
  expect(overflow.page).toBe(0);
  expect(overflow.thread).toBe(0);

  // The thread is a flex column and a card sets `overflow: hidden`, which zeroes
  // a flex item's automatic minimum size. Without `flex: none` a full thread
  // crushed these to a sliver of clipped text — vertical, so the horizontal
  // checks above sailed past it. Assert each card still fits its own header.
  const crushed = await page.evaluate(() =>
    Array.from(document.querySelectorAll('[data-testid="session-tool-card"]')).filter((card) => {
      const head = card.querySelector("button") as HTMLElement;
      return card.getBoundingClientRect().height + 0.5 < head.scrollHeight;
    }).length,
  );
  expect(crushed).toBe(0);

  // Collapsed, the summary stays visible and completed tool rows hide behind it;
  // thoughts and plans stay hidden too.
  await page.getByTestId("session-turn-work-summary").click();
  await expect(page.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "false");
  await expect(page.getByTestId("session-tool-card")).toHaveCount(0);
  await expect(page.getByTestId("session-plan")).toHaveCount(0);
  await expect(page.getByTestId("session-thinking")).toHaveCount(0);
  await expect(page.getByTestId("session-turn-work-summary")).toContainText(
    "Edited 1 file · ran 1 command · 1 failed",
  );
});

test("hard-wrapped agent markdown prose stays inside the thread width", async ({ page }) => {
  const LONG = "b".repeat(400);
  let nextCursor = 0;

  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    const send = (event: SessionServerEvent) =>
      socket.send(sessionEventJson(nextCursor++, event));

    socket.send(sessionSnapshotJson({ cursor: nextCursor, model: "auto", turnState: "idle" }));
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as { type?: string; text?: string };
      if (event.type !== "prompt") return;
      send({
        type: "message",
        role: "agent",
        text: [
          "SaySo agent turns often arrive hard-wrapped at eighty columns so the",
          "source reads fine in a terminal but should flow as one chat paragraph.",
          "",
          "```txt",
          LONG,
          "```",
          "",
          "| Alpha | Beta |",
          "| --- | --- |",
          `| ${LONG} | two |`,
        ].join("\n"),
      });
      send({ type: "turn_end", stopReason: "end_turn" });
    });
  });

  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });

  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible();
  await page.getByLabel("Message").fill("Show wrapped prose");
  await page.getByLabel("Message").press("Enter");

  const reply = page.getByTestId("session-message-agent").last();
  await expect(reply).toContainText("flow as one chat paragraph");
  await expect(reply.locator(".md-para")).toHaveCount(1);
  await expect(reply.locator(".md-para").first()).toContainText(
    "SaySo agent turns often arrive hard-wrapped at eighty columns so the source reads fine in a terminal but should flow as one chat paragraph.",
  );

  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    const thread = document.querySelector('[data-testid="session-thread"]') as HTMLElement;
    const code = document.querySelector(".md-block") as HTMLElement;
    const table = document.querySelector(".md-table-wrap") as HTMLElement;
    return {
      page: doc.scrollWidth - doc.clientWidth,
      thread: thread.scrollWidth - thread.clientWidth,
      codeScrollable: code ? code.scrollWidth > code.clientWidth : false,
      tableScrollable: table ? table.scrollWidth > table.clientWidth : false,
    };
  });
  expect(overflow.page).toBe(0);
  expect(overflow.thread).toBe(0);
  expect(overflow.codeScrollable).toBe(true);
  expect(overflow.tableScrollable).toBe(true);
});

test("an error turn ends visibly with recovery guidance", async ({ page }) => {
  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });
  await mockScriptedSession(page, "Cursor", { errorTurn: true });

  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-chat")).toBeVisible();
  await page.getByLabel("Message").fill("Trigger an error");
  await page.getByLabel("Message").press("Enter");

  await expect(
    page.getByTestId("session-note-error").filter({ hasText: "stopped without a response" }),
  ).toHaveText("The agent stopped without a response. Check the selected model or try again.");
  await expect(page.getByTestId("session-head")).not.toHaveAttribute("data-state", "working");
});
