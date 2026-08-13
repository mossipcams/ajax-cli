// Orchestration chat feature-flag and routing characterization.
// OC-IOS-COMPOSER-BAND coverage lives in session-chat.test.ts (geometry / keyboard band).

import { test, expect, type Page } from "@playwright/test";
import { COCKPIT_FIXTURE, DETAIL_FIXTURE, mockFetch, mockTerminalWebSocket } from "./fixtures";

const ORCHESTRATION_CHAT_KEY = "ajax.web.session.orchestrationChat";

async function boot(page: Page, flag: "true" | "false" | "unset", hash = "/app.html#/") {
  await mockFetch(page);
  if (flag !== "unset") {
    await page.addInitScript(
      ([key, value]: [string, string]) => {
        localStorage.setItem(key, value);
      },
      [ORCHESTRATION_CHAT_KEY, flag] as [string, string],
    );
  }
  await page.goto(hash);
}

async function bootSessionChat(page: Page) {
  await mockFetch(page);
  await page.addInitScript(
    (key: string) => {
      localStorage.setItem(key, "true");
      class QuietSocket extends EventTarget {
        readyState = 1;
        constructor(public url: string) {
          super();
          setTimeout(() => {
            this.dispatchEvent(new Event("open"));
            this.dispatchEvent(
              new MessageEvent("message", {
                data: JSON.stringify({ type: "ready", model: "auto" }),
              }),
            );
          }, 10);
        }
        send() {}
        close() {
          this.readyState = 3;
        }
      }
      (globalThis as unknown as { WebSocket: unknown }).WebSocket = QuietSocket;
    },
    ORCHESTRATION_CHAT_KEY,
  );
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-head")).toBeVisible({ timeout: 10_000 });
}

test("OC-FLAG-DEFAULT-OFF keeps dashboard new-task sheet and terminal task open", async ({
  page,
}) => {
  await boot(page, "unset");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  await page.getByRole("button", { name: "New" }).click();
  await expect(page.getByTestId("new-task-sheet")).toBeVisible();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByTestId("new-task-sheet")).toHaveCount(0);

  await mockTerminalWebSocket(page);
  await page.getByText("web/fix-login").click();
  await expect(page).toHaveURL(/#\/t\/web%2Ffix-login$/);
  await expect(page.getByTestId("task-terminal-panel")).toBeVisible({ timeout: 10_000 });
});

test("OC-FLAG-ON-STARTER opens session starter from New and /session", async ({ page }) => {
  await boot(page, "true");
  await page.getByRole("button", { name: "New" }).click();
  await expect(page).toHaveURL(/#\/session$/);
  await expect(page.getByTestId("session-starter")).toBeVisible();

  await page.goto("/app.html#/session");
  await expect(page.getByTestId("session-starter")).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Mobile navigation" })).toHaveCount(0);
});

test("OC-FLAG-OFF-SESSION-REDIRECT sends session hashes to dashboard", async ({ page }) => {
  await boot(page, "false", "/app.html#/session/web%2Ffix-login");
  await expect(page).toHaveURL(/#\/$/);
  await expect(page.getByTestId("session-starter")).toHaveCount(0);
});

test("OC-FLAG-ON-OPEN-TASK-SESSION routes task open to session hash", async ({ page }) => {
  await boot(page, "true");
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  await page.getByText("web/fix-login").click();
  await expect(page).toHaveURL(/#\/session\/web%2Ffix-login$/);
});

test("OC-FLAG-OFF-OPEN-TASK-TERMINAL keeps legacy terminal route", async ({ page }) => {
  await boot(page, "false");
  await mockTerminalWebSocket(page);
  await expect(page.getByText("web/fix-login")).toBeVisible({ timeout: 10_000 });
  await page.getByText("web/fix-login").click();
  await expect(page).toHaveURL(/#\/t\/web%2Ffix-login$/);
  await expect(page.getByTestId("task-terminal-panel")).toBeVisible({ timeout: 10_000 });
});

test("OC-DIFF-FROM-SESSION-BACK returns to session route when flag on", async ({ page }) => {
  await mockFetch(page, {
    __detail__: { ...DETAIL_FIXTURE, agent: "cursor" },
  });
  await page.addInitScript((key: string) => {
    localStorage.setItem(key, "true");
  }, ORCHESTRATION_CHAT_KEY);
  await page.goto("/app.html#/session/web%2Ffix-login");
  await page.goto("/app.html#/t/web%2Ffix-login/diff");
  await expect(page.getByTestId("diff-review")).toBeVisible({ timeout: 10_000 });
  await page.getByRole("button", { name: "Back" }).click();
  await expect(page).toHaveURL(/#\/session\/web%2Ffix-login$/);
});

test("OC-IOS-ENTER-TO-SEND-NO-SEND-BUTTON hides Send on mobile session composer", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await bootSessionChat(page);
  await expect(page.getByRole("button", { name: "Send" })).toHaveCount(0);
  const composer = page.getByTestId("session-composer").locator("textarea");
  await expect(composer).toHaveAttribute("enterkeyhint", "send");
});

test("OC-IOS-NO-DASHBOARD-CHROME hides bottom nav on session routes", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await boot(page, "true", "/app.html#/session");
  await expect(page.getByTestId("session-starter")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByRole("navigation", { name: "Mobile navigation" })).toHaveCount(0);
  await expect(page.getByTestId("cockpit-chrome")).toHaveCount(0);
});

test("OC-TERM-ESCAPE-SHEET opens terminal overlay from session task details", async ({ page }) => {
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.addInitScript((key: string) => {
    localStorage.setItem(key, "true");
    class QuietSocket extends EventTarget {
      readyState = 1;
      constructor(public url: string) {
        super();
        setTimeout(() => {
          this.dispatchEvent(new Event("open"));
          this.dispatchEvent(
            new MessageEvent("message", {
              data: JSON.stringify({ type: "ready", model: "auto" }),
            }),
          );
        }, 10);
      }
      send() {}
      close() {
        this.readyState = 3;
      }
    }
    (globalThis as unknown as { WebSocket: unknown }).WebSocket = QuietSocket;
  }, ORCHESTRATION_CHAT_KEY);
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-head")).toBeVisible({ timeout: 10_000 });
  await page.getByTestId("session-details").click();
  await page.getByTestId("session-terminal-toggle").click();
  await expect(page.getByTestId("session-terminal-sheet")).toBeVisible();
  await expect(page.getByTestId("task-terminal-panel")).toBeVisible();
});

test("OC-TERM-HASH-T-NOT-CHAT keeps #/t on legacy terminal host", async ({ page }) => {
  await mockFetch(page);
  await mockTerminalWebSocket(page);
  await page.addInitScript((key: string) => {
    localStorage.setItem(key, "true");
  }, ORCHESTRATION_CHAT_KEY);
  await page.goto("/app.html#/t/web%2Ffix-login");
  await expect(page.getByTestId("task-detail")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("session-chat")).toHaveCount(0);
  await expect(page.getByTestId("task-terminal-panel")).toBeVisible();
});

test("OC-CANCEL-EMPTY-ENTER-NOOP does not stop an in-flight turn with empty Enter", async ({
  page,
}) => {
  await mockFetch(page);
  await page.addInitScript((key: string) => {
    localStorage.setItem(key, "true");
    class BusySocket extends EventTarget {
      readyState = 1;
      sent: string[] = [];
      constructor(public url: string) {
        super();
        setTimeout(() => {
          this.dispatchEvent(new Event("open"));
          this.dispatchEvent(
            new MessageEvent("message", {
              data: JSON.stringify({ type: "ready", model: "auto" }),
            }),
          );
          this.dispatchEvent(
            new MessageEvent("message", {
              data: JSON.stringify({ type: "message", role: "agent", text: "working" }),
            }),
          );
        }, 10);
      }
      send(data: string) {
        this.sent.push(data);
      }
      close() {
        this.readyState = 3;
      }
    }
    (globalThis as unknown as { WebSocket: unknown }).WebSocket = BusySocket;
    (globalThis as unknown as { __lastSessionSocket?: BusySocket }).__lastSessionSocket =
      undefined;
    const ProxySocket = new Proxy(BusySocket, {
      construct(target, args: [string]) {
        const socket = new target(args[0]);
        (globalThis as unknown as { __lastSessionSocket?: BusySocket }).__lastSessionSocket =
          socket;
        return socket;
      },
    });
    (globalThis as unknown as { WebSocket: unknown }).WebSocket = ProxySocket;
  }, ORCHESTRATION_CHAT_KEY);
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-head")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("session-head")).toHaveAttribute("data-state", "working");
  const composer = page.getByTestId("session-composer").locator("textarea");
  await composer.focus();
  await composer.press("Enter");
  const sent = await page.evaluate(
    () =>
      (globalThis as unknown as { __lastSessionSocket?: { sent: string[] } })
        .__lastSessionSocket?.sent ?? [],
  );
  expect(sent.some((payload) => payload.includes('"type":"cancel"'))).toBe(false);
});

test("OC-CANCEL-ENTER-AGAIN sends keepQueue cancel after a follow-up is queued", async ({
  page,
}) => {
  await mockFetch(page);
  await page.addInitScript((key: string) => {
    localStorage.setItem(key, "true");
    class BusySocket extends EventTarget {
      readyState = 1;
      sent: string[] = [];
      constructor(public url: string) {
        super();
        setTimeout(() => {
          this.dispatchEvent(new Event("open"));
          this.dispatchEvent(
            new MessageEvent("message", {
              data: JSON.stringify({ type: "ready", model: "auto" }),
            }),
          );
          this.dispatchEvent(
            new MessageEvent("message", {
              data: JSON.stringify({ type: "message", role: "agent", text: "working" }),
            }),
          );
        }, 10);
      }
      send(data: string) {
        this.sent.push(data);
      }
      close() {
        this.readyState = 3;
      }
    }
    (globalThis as unknown as { WebSocket: unknown }).WebSocket = new Proxy(BusySocket, {
      construct(target, args: [string]) {
        const socket = new target(args[0]);
        (globalThis as unknown as { __lastSessionSocket?: BusySocket }).__lastSessionSocket =
          socket;
        return socket;
      },
    });
  }, ORCHESTRATION_CHAT_KEY);
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-head")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByTestId("session-head")).toHaveAttribute("data-state", "working");
  const composer = page.getByTestId("session-composer").locator("textarea");
  await expect(composer).toHaveAttribute("placeholder", "Sends after this turn…");
  await composer.fill("follow-up");
  await composer.press("Enter");
  await expect(composer).toHaveAttribute(
    "placeholder",
    /Enter again to stop and send/i,
  );
  await composer.fill("");
  await composer.press("Enter");
  const sent = await page.evaluate(
    () =>
      (globalThis as unknown as { __lastSessionSocket?: { sent: string[] } })
        .__lastSessionSocket?.sent ?? [],
  );
  expect(
    sent.some(
      (payload) =>
        payload.includes('"type":"cancel"') && payload.includes('"keepQueue":true'),
    ),
  ).toBe(true);
});

test("OC-PERM-DISCONNECTED-STAYS keeps decision controls blocked when socket is down", async ({
  page,
}) => {
  await mockFetch(page);
  await page.addInitScript((key: string) => {
    localStorage.setItem(key, "true");
    class DecisionSocket extends EventTarget {
      readyState = 1;
      constructor(public url: string) {
        super();
        setTimeout(() => {
          this.dispatchEvent(new Event("open"));
          this.dispatchEvent(
            new MessageEvent("message", {
              data: JSON.stringify({ type: "ready", model: "auto" }),
            }),
          );
          this.dispatchEvent(
            new MessageEvent("message", {
              data: JSON.stringify({
                type: "permission_request",
                requestId: "42",
                title: "Run tests?",
              }),
            }),
          );
        }, 10);
      }
      send() {}
      close() {
        this.readyState = 3;
        this.dispatchEvent(new CloseEvent("close"));
      }
    }
    (globalThis as unknown as { WebSocket: unknown }).WebSocket = new Proxy(DecisionSocket, {
      construct(target, args: [string]) {
        const socket = new target(args[0]);
        (globalThis as unknown as { __lastSessionSocket?: DecisionSocket }).__lastSessionSocket =
          socket;
        return socket;
      },
    });
  }, ORCHESTRATION_CHAT_KEY);
  await page.goto("/app.html#/session/web%2Ffix-login");
  const decision = page.getByTestId("session-decision");
  await expect(decision).toBeVisible({ timeout: 10_000 });
  await page.evaluate(() => {
    (
      globalThis as unknown as { __lastSessionSocket?: { close: () => void } }
    ).__lastSessionSocket?.close();
  });
  await expect(page.getByTestId("session-head-offline")).toBeVisible({ timeout: 10_000 });
  await expect(decision).toBeVisible();
  await expect
    .poll(async () => decision.getByRole("button", { name: "Approve" }).isDisabled(), {
      timeout: 5_000,
    })
    .toBe(true);
});

test("OC-PROMPT-DISCONNECTED-REFUSED ignores send while reconnecting", async ({ page }) => {
  await mockFetch(page);
  await page.addInitScript((key: string) => {
    localStorage.setItem(key, "true");
    class DeadSocket extends EventTarget {
      readyState = 3;
      sent: string[] = [];
      constructor(public url: string) {
        super();
      }
      send(data: string) {
        this.sent.push(data);
      }
      close() {
        this.readyState = 3;
      }
    }
    (globalThis as unknown as { WebSocket: unknown }).WebSocket = new Proxy(DeadSocket, {
      construct(target, args: [string]) {
        const socket = new target(args[0]);
        (globalThis as unknown as { __lastSessionSocket?: DeadSocket }).__lastSessionSocket =
          socket;
        return socket;
      },
    });
  }, ORCHESTRATION_CHAT_KEY);
  await page.goto("/app.html#/session/web%2Ffix-login");
  await expect(page.getByTestId("session-head-offline")).toBeVisible({ timeout: 10_000 });
  const composer = page.getByTestId("session-composer").locator("textarea");
  await composer.fill("hello");
  await composer.press("Enter");
  const sent = await page.evaluate(
    () =>
      (globalThis as unknown as { __lastSessionSocket?: { sent: string[] } })
        .__lastSessionSocket?.sent ?? [],
  );
  expect(sent).toHaveLength(0);
});

test("dashboard still lists cockpit cards when flag on", async ({ page }) => {
  await boot(page, "true");
  await expect(page.getByText(COCKPIT_FIXTURE.cards[0].title)).toBeVisible({
    timeout: 10_000,
  });
});
