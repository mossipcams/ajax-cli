// Orchestration session chat in a real browser. The ACP WebSocket is replaced
// before the app boots with a scripted socket that replays a realistic Cursor
// turn — reasoning, tool calls, prose, a permission request — so the live head
// and the settled transcript can be exercised without a Cursor process.

import { test, expect, type Page } from "@playwright/test";
import { mockFetch } from "./fixtures";

const SHOT_DIR = process.env.AJAX_SHOT_DIR;

function shotPath(testInfo: { outputPath: (name: string) => string }, name: string): string {
  return SHOT_DIR ? `${SHOT_DIR}/${name}` : testInfo.outputPath(name);
}

const ORCHESTRATION_CHAT_KEY = "ajax.web.session.orchestrationChat";
const SESSION_URL = "/app.html#/session/web%2Ffix-login";

type Scripted = { delay: number; payload: Record<string, unknown> }[];

async function bootSession(page: Page, script: Scripted) {
  await mockFetch(page);
  await page.addInitScript(
    ([key, events]: [string, Scripted]) => {
      localStorage.setItem(key, "true");

      class ScriptedSocket extends EventTarget {
        readyState = 1;
        constructor(public url: string) {
          super();
          setTimeout(() => {
            this.dispatchEvent(new Event("open"));
            this.emit({ type: "ready" });
            for (const step of events) {
              setTimeout(
                () => this.emit(step.payload as Record<string, unknown>),
                step.delay,
              );
            }
          }, 10);
        }
        emit(payload: Record<string, unknown>) {
          this.dispatchEvent(
            new MessageEvent("message", { data: JSON.stringify(payload) }),
          );
        }
        send() {}
        close() {
          this.readyState = 3;
        }
      }
      (globalThis as unknown as { WebSocket: unknown }).WebSocket = new Proxy(
        ScriptedSocket,
        {
          construct(target, args: [string]) {
            const socket = new target(args[0]);
            // Let a test drive extra output after the scripted turn.
            (globalThis as unknown as { __ajaxEmit?: unknown }).__ajaxEmit = (
              payload: Record<string, unknown>,
            ) => socket.emit(payload);
            return socket;
          },
        },
      );
    },
    [ORCHESTRATION_CHAT_KEY, script] as [string, Scripted],
  );
  await page.goto(SESSION_URL);
  await expect(page.getByTestId("session-head")).toBeVisible({ timeout: 10_000 });
}

const WORKING_TURN: Scripted = [
  { delay: 20, payload: { type: "message", role: "thought", text: "The failure is in the session router, not the guard itself." } },
  { delay: 40, payload: { type: "tool_call", callId: "c1", title: "Read session router", kind: "read", status: "completed", locations: ["/repo/crates/ajax-web/src/runtime/task_routes/live.rs"] } },
  { delay: 60, payload: { type: "tool_call", callId: "c2", title: "Search for prepare_task_session", kind: "search", status: "completed", locations: ["/repo/crates/ajax-web/src"] } },
  {
    delay: 80,
    payload: {
      type: "message",
      role: "agent",
      text: "The route rejects the attach before the worktree check runs, so a missing worktree surfaces as **NotOrchestrationChat**.\n\nTwo things need changing:\n\n- reorder the guards in `prepare_task_session`\n- return `WorktreeMissing` so the browser can offer a repair\n\n```rust\nif !task.worktree_path.exists() {\n    return Err(SessionRouteError::WorktreeMissing);\n}\n```",
    },
  },
  { delay: 100, payload: { type: "plan", entries: [{ content: "Reorder the guards in prepare_task_session", status: "completed" }, { content: "Return WorktreeMissing for a missing worktree", status: "in_progress" }, { content: "Cover both orders with a slice test", status: "pending" }] } },
  { delay: 120, payload: { type: "tool_call", callId: "c3", title: "Edit web_session.rs", kind: "edit", status: "in_progress", locations: ["/repo/crates/ajax-web/src/slices/web_session.rs"] } },
];

const DECISION_TURN: Scripted = [
  ...WORKING_TURN,
  { delay: 140, payload: { type: "turn_end", stopReason: "end_turn" } },
  { delay: 160, payload: { type: "permission_request", requestId: "42", title: "Run cargo nextest run -p ajax-web?", detail: "The agent wants to run the ajax-web slice tests in this worktree." } },
];

test("live head reports the running tool while the transcript settles", async ({ page }) => {
  await bootSession(page, WORKING_TURN);

  const head = page.getByTestId("session-head");
  await expect(head).toHaveAttribute("data-state", "working");
  await expect(page.getByTestId("session-head-tool")).toContainText("Edit web_session.rs");
  await expect(page.getByTestId("session-head-tool")).toContainText("…/slices/web_session.rs");

  // Prose arrives as real markdown, not a raw blob.
  await expect(page.locator(".session-reply pre code")).toContainText("WorktreeMissing");
  await expect(page.locator(".session-reply li").first()).toContainText("prepare_task_session");
  await expect(page.getByTestId("session-plan")).toContainText("Cover both orders");

  // Tool calls are labelled rows, never a JSON dump.
  await expect(page.getByTestId("session-tools").first()).toContainText("Read session router");
  await expect(page.getByText("sessionUpdate")).toHaveCount(0);
});

test("a permission request takes over the head as the decision", async ({ page }) => {
  await bootSession(page, DECISION_TURN);

  const decision = page.getByTestId("session-decision");
  await expect(decision).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("session-head")).toHaveAttribute("data-state", "decision");
  await expect(decision).toContainText("cargo nextest run");
  await expect(decision.getByRole("button", { name: "Approve" })).toBeVisible();

  // The decision must be reachable without scrolling — it is the whole point.
  const box = await decision.boundingBox();
  const viewport = page.viewportSize();
  expect(box).not.toBeNull();
  expect(box!.y + box!.height).toBeLessThan(viewport!.height);
});

test("the transcript holds its position while new output streams in", async ({ page }) => {
  // Enough turns to overflow the transcript on a desktop viewport too.
  const long: Scripted = [
    ...WORKING_TURN,
    ...Array.from({ length: 8 }, (_, index) => ({
      delay: 140 + index * 5,
      payload: {
        type: "message",
        role: "agent",
        text: `Checked sibling guard ${index + 1} and it already returns the right error for a missing worktree.`,
      },
    })),
  ];
  await bootSession(page, long);
  const thread = page.getByTestId("session-thread");
  await expect(page.getByTestId("session-plan")).toBeVisible();
  await expect(page.getByText("Checked sibling guard 8")).toBeVisible();

  // The transcript must be tall enough that scrolling away is meaningful, and
  // the scripted turn lands asynchronously — poll rather than sample once.
  await expect
    .poll(() => thread.evaluate((node) => node.scrollHeight - node.clientHeight))
    .toBeGreaterThan(0);
  // WebKit delivers the scroll event asynchronously; drive it explicitly and
  // let React commit before new output arrives, or the race decides the result.
  await thread.evaluate(async (node) => {
    node.scrollTop = 0;
    node.dispatchEvent(new Event("scroll"));
    await new Promise((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(resolve)),
    );
  });
  const before = await thread.evaluate((node) => node.scrollTop);

  // New output lands while the operator is reading history.
  await page.evaluate(() => {
    const emit = (globalThis as unknown as { __ajaxEmit: (p: unknown) => void }).__ajaxEmit;
    emit({ type: "message", role: "agent", text: "Also patched the sibling guard." });
  });
  await expect(page.getByTestId("session-jump")).toBeVisible();

  const after = await thread.evaluate((node) => node.scrollTop);
  expect(after).toBe(before);

  await page.getByTestId("session-jump").click();
  await expect(page.getByTestId("session-jump")).toBeHidden();
  expect(await thread.evaluate((node) => node.scrollTop)).toBeGreaterThan(before);
});

test("session chat screenshots", async ({ page }, testInfo) => {
  await bootSession(page, WORKING_TURN);
  await expect(page.getByTestId("session-plan")).toBeVisible();
  await page.waitForTimeout(700);
  await page.screenshot({
    path: shotPath(testInfo, `session-working-${testInfo.project.name}.png`),
    fullPage: false,
  });

  await page.getByTestId("session-details").click();
  await expect(page.getByTestId("session-task-panel")).toBeVisible();
  await page.waitForTimeout(500);
  await page.screenshot({
    path: shotPath(testInfo, `session-details-${testInfo.project.name}.png`),
    fullPage: false,
  });
});

test("decision screenshot", async ({ page }, testInfo) => {
  await bootSession(page, DECISION_TURN);
  await expect(page.getByTestId("session-decision")).toBeVisible({ timeout: 10_000 });
  await page.waitForTimeout(700);
  await page.screenshot({
    path: shotPath(testInfo, `session-decision-${testInfo.project.name}.png`),
    fullPage: false,
  });
});

test("starter screenshot", async ({ page }, testInfo) => {
  await mockFetch(page);
  await page.addInitScript((key: string) => localStorage.setItem(key, "true"), ORCHESTRATION_CHAT_KEY);
  await page.goto("/app.html#/session");
  await expect(page.getByTestId("session-starter")).toBeVisible({ timeout: 10_000 });
  await page.waitForTimeout(500);
  await page.screenshot({
    path: shotPath(testInfo, `session-starter-${testInfo.project.name}.png`),
    fullPage: false,
  });
});

test("attention head screenshot", async ({ page }, testInfo) => {
  // No events: the fixture task is `waiting`, so the head sits in `attention`
  // and must offer the task's actions, not just describe the problem.
  await bootSession(page, []);
  await expect(page.getByTestId("session-head")).toHaveAttribute("data-state", "attention");
  await expect(page.getByTestId("session-head-actions")).toBeVisible();
  await page.waitForTimeout(400);
  await page.screenshot({
    path: shotPath(testInfo, `session-attention-${testInfo.project.name}.png`),
    fullPage: false,
  });
});

test("decision with typed draft screenshot", async ({ page }, testInfo) => {
  // The Send affordance must stay findable while a decision holds the accent —
  // this is the enabled state, which the at-rest captures cannot show.
  await bootSession(page, DECISION_TURN);
  await expect(page.getByTestId("session-decision")).toBeVisible({ timeout: 10_000 });
  await page.getByLabel("Message").fill("Run only the web slice instead");
  await expect(page.getByRole("button", { name: "Send" })).toBeEnabled();
  await page.waitForTimeout(400);
  await page.screenshot({
    path: shotPath(testInfo, `session-decision-typed-${testInfo.project.name}.png`),
    fullPage: false,
  });
});
