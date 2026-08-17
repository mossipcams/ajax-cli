import { expect, test, type Page } from "@playwright/test";
import { COCKPIT_FIXTURE, DETAIL_FIXTURE, mockFetch } from "./fixtures";

const HARNESSES = ["Cursor", "Codex", "Claude", "Pi"] as const;

async function mockScriptedSession(
  page: Page,
  harness: (typeof HARNESSES)[number],
  options: { errorTurn?: boolean } = {},
) {
  const transcript: Array<Record<string, unknown>> = [];
  let turn = 0;
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as {
        type?: string;
        text?: string;
        clientMessageId?: string;
      };
      if (event.type !== "prompt" || !event.text) return;
      if (event.clientMessageId) {
        socket.send(
          JSON.stringify({ type: "prompt_accepted", clientMessageId: event.clientMessageId }),
        );
      }
      turn += 1;
      if (options.errorTurn) {
        const events = [
          { type: "message", role: "user", text: event.text },
          { type: "turn_end", stopReason: "error" },
        ];
        transcript.push(...events);
        socket.send(JSON.stringify(events[1]));
        return;
      }
      const reply =
        turn === 1 ? `${harness}-ONE`
        : turn === 2 ? `### ${harness}-TWO\n\n- **bold** and \`code\`\n- second bullet\n\n1. first\n2. second\n\n\`\`\`txt\n${harness}-CODE\n\`\`\``
        : `${harness}-THREE: ${harness}-TWO`;
      const events = [
        { type: "message", role: "user", text: event.text },
        { type: "message", role: "agent", text: reply },
        { type: "turn_end", stopReason: "end_turn" },
      ];
      transcript.push(...events);
      for (const next of events.slice(1)) socket.send(JSON.stringify(next));
    });
    for (const event of transcript) socket.send(JSON.stringify(event));
    socket.send(JSON.stringify({ type: "ready", model: "auto", busy: false }));
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

/** One turn carrying everything ACP separates: reasoning, a plan, a tool call
 * with a diff, and a tool call with command output. The long diff line is the
 * point of the width assertions — `white-space: pre` inside a flex column is
 * exactly how a code block starts panning the whole phone surface sideways. */
async function mockTypedTurn(page: Page) {
  const LONG = "a".repeat(400);
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socket.onMessage((message) => {
      if (typeof message !== "string") return;
      const event = JSON.parse(message) as { type?: string; text?: string };
      if (event.type !== "prompt") return;
      const events = [
        { type: "message", role: "thought", text: "Deciding where the port is set" },
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
          status: "completed",
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
      for (const next of events) socket.send(JSON.stringify(next));
    });
    socket.send(JSON.stringify({ type: "ready", model: "auto", busy: false }));
  });
}

test("a turn renders its tool calls, diff, plan and reasoning in place", async ({ page }) => {
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

  // Two calls, merged by id — the update revised the edit rather than adding a row.
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
