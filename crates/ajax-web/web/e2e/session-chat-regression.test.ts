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
      const event = JSON.parse(message) as { type?: string; text?: string };
      if (event.type !== "prompt" || !event.text) return;
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
