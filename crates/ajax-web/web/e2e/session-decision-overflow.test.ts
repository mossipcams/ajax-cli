import { expect, test, type Page } from "@playwright/test";
import { DETAIL_FIXTURE, mockFetch, sessionEventJson, sessionSnapshotJson } from "./fixtures";

const CHAT_DETAIL = { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true };

const QUESTIONS = [
  {
    header: "Eval set",
    question: "Which evaluation set should the benchmark run against before we compare the two checkpoints?",
    options: [
      { label: "Held-out", description: "The 2k held-out prompts that never touched training." },
      { label: "Regression", description: "The curated regression suite from the last three releases." },
      { label: "Both", description: "Run both sets and report them side by side." },
    ],
  },
  {
    header: "Hardware",
    question: "Which GPU host should run the job, given the current queue on each machine?",
    options: [
      { label: "sayso-llm", description: "Local host, idle right now, slower per step." },
      { label: "Cloud A100", description: "Faster, but costs money and needs a quota bump." },
    ],
  },
  {
    header: "Metrics",
    question: "Which metrics should the report include? Pick every one that matters for the decision.",
    multiSelect: true,
    options: [{ label: "Accuracy" }, { label: "Latency" }, { label: "Cost" }, { label: "Refusals" }],
  },
  {
    header: "Report",
    question: "Where should the final comparison report be posted once the run finishes?",
    options: [
      { label: "PR comment", description: "Attach the table to the open pull request." },
      { label: "Issue", description: "Post it on the tracking issue." },
    ],
  },
];

function askUserQuestionSchema(questions: typeof QUESTIONS) {
  const properties: Record<string, unknown> = {};
  questions.forEach((question, index) => {
    const labels = question.options.map((option) => option.label);
    properties[`question_${index}`] = question.multiSelect
      ? {
          type: "string",
          title: question.header,
          description: `${question.question} (comma-separated: ${labels.join(", ")})`,
        }
      : {
          type: "string",
          title: question.header,
          description: question.question,
          oneOf: question.options.map((option) => ({
            const: option.label,
            title: option.label,
            ...("description" in option ? { description: option.description } : {}),
          })),
        };
    properties[`question_${index}_other`] = {
      type: "string",
      title: "Other",
      description: "Type your own answer (optional).",
    };
  });
  return { type: "object", properties, required: [] };
}

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

async function expectElicitationActionsInViewport(page: Page): Promise<void> {
  await viewportCheck(page, page.getByRole("button", { name: "Accept" }));
  await viewportCheck(page, page.getByRole("button", { name: "Decline" }));
  await viewportCheck(page, page.getByRole("button", { name: "Cancel" }));
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

test("#1665 shows every AskUserQuestion question and keeps Accept/Decline/Cancel in the viewport", async ({
  page,
}) => {
  await openChatWithEvents(page, [
    {
      type: "elicitation_request",
      requestId: "e1",
      message: "Claude has questions for you",
      schema: askUserQuestionSchema(QUESTIONS),
    },
  ]);

  const panel = page.getByTestId("session-elicitation");
  await expect(panel).toBeVisible();

  await expect(page.getByText(QUESTIONS[0].question)).toBeInViewport();
  for (const question of QUESTIONS) {
    await expect(page.getByText(question.question, { exact: false })).toBeVisible();
  }
  await expect(page.getByText("Accuracy, Latency, Cost, Refusals", { exact: false })).toBeVisible();

  const fields = page.locator(".session-elicitation-fields");
  const state = await scrollState(fields);
  expect(state.overflowY).toBe("auto");
  expect(state.scrollable).toBe(true);

  await expectElicitationActionsInViewport(page);

  await fields.evaluate((node) => {
    node.scrollTop = node.scrollHeight;
  });
  await expect(page.getByText(QUESTIONS[3].question)).toBeInViewport();
  await expectElicitationActionsInViewport(page);
});

test("#1665 leaves the transcript visible under a one-question form", async ({ page }) => {
  await openChatWithEvents(page, [
    {
      type: "elicitation_request",
      requestId: "e2",
      message: QUESTIONS[1].question,
      schema: askUserQuestionSchema([QUESTIONS[1]]),
    },
  ]);

  await expect(page.getByTestId("session-elicitation")).toBeVisible();
  await expectElicitationActionsInViewport(page);

  const thread = page.locator(".session-thread");
  const height = await thread.evaluate((node) => node.getBoundingClientRect().height);
  expect(height).toBeGreaterThan(80);
});
