import { expect, test, type Page } from "@playwright/test";
import { HISTORY_REVEAL_BATCH } from "../src/features/chat/conversation/historyWindow";
import { LIVE_EDGE_PIN_BAND_PX } from "../src/shared/lib/sessionViewport";
import { DETAIL_FIXTURE, mockFetch, sessionEventJson, sessionSnapshotJson } from "./fixtures";

const TOTAL_TURNS = 100;
const TAIL_TURNS = 40;
const TAIL_FROM = (TOTAL_TURNS - TAIL_TURNS) * 2;
const transcript = Array.from({ length: TOTAL_TURNS }, (_, turn) => [
  { type: "message", role: "user", text: `Question ${turn}`, itemId: `u${turn}` },
  {
    type: "message",
    role: "agent",
    text: `Answer ${turn}. ${"Padding to overflow the transcript. ".repeat(4)}`,
    itemId: `a${turn}`,
  },
]).flat().map((event, cursor) =>
  JSON.parse(sessionEventJson(cursor, event)) as Record<string, unknown> & { cursor: number },
);

async function openSession(page: Page, legacy = false, shortTail = false) {
  const fixturePage = {
    addInitScript: async (script: (routes: Record<string, unknown>) => void, routes: Record<string, unknown>) => {
      await page.addInitScript({ content: `
        (() => {
          const nativeFetch = window.fetch.bind(window);
          (${script.toString()})(${JSON.stringify(routes)});
          const mockedFetch = window.fetch;
          window.fetch = (input, init) => {
            const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
            return new URL(url, location.href).pathname.endsWith("/session/transcript")
              ? nativeFetch(input, init)
              : mockedFetch(input, init);
          };
        })();
      ` });
    },
  } as Page;
  await mockFetch(fixturePage, {
    __detail__: { ...DETAIL_FIXTURE, agent: "Cursor", session_capable: true },
  });
  await page.addInitScript(() => {
    localStorage.setItem("ajax.web.session.orchestrationChat", "true");
  });
  const socketUrls: string[] = [];
  const tailTurns = shortTail ? 2 : TAIL_TURNS;
  const tailFrom = shortTail ? (TOTAL_TURNS - tailTurns) * 2 : TAIL_FROM;
  const bodyPadding = shortTail ? "" : " Padding to overflow the transcript. ";
  const tailTranscript = Array.from({ length: tailTurns }, (_, turn) => {
    const index = TOTAL_TURNS - tailTurns + turn;
    return [
      { type: "message", role: "user" as const, text: `Question ${index}`, itemId: `u${index}` },
      {
        type: "message",
        role: "agent" as const,
        text: `Answer ${index}.${bodyPadding.repeat(4)}`,
        itemId: `a${index}`,
      },
    ];
  }).flat().map((event, offset) =>
    JSON.parse(sessionEventJson(tailFrom + offset, event)) as Record<string, unknown>,
  );
  await page.routeWebSocket(/\/api\/tasks\/.*\/session/, (socket) => {
    socketUrls.push(socket.url());
    socket.send(sessionSnapshotJson({
      reset: true,
      cursor: legacy ? 80 : transcript.length,
      transcript: legacy
        ? transcript.slice(0, 80)
        : shortTail
          ? tailTranscript
          : transcript.slice(TAIL_FROM),
      ...(!legacy ? { transcriptFrom: shortTail ? tailFrom : TAIL_FROM, transcriptFloor: 0 } : {}),
    }));
  });

  await page.goto("/app.html#/session/web%2Ffix-login");
  if (shortTail) {
    await expect(page.getByTestId("session-message-agent").last()).toContainText(`Answer ${TOTAL_TURNS - 1}`);
  } else {
    await expect(page.getByTestId("session-message-agent")).toHaveCount(tailTurns);
  }
  return socketUrls;
}

async function routeHistory(page: Page, failFirst = false) {
  const requests: URL[] = [];
  await page.route("**/api/tasks/**/session/transcript*", async (route) => {
    expect(route.request().method()).toBe("GET");
    const url = new URL(route.request().url());
    requests.push(url);
    if (failFirst && requests.length === 1) {
      await route.fulfill({ status: 500, json: { error: "Temporary failure" } });
      return;
    }
    const before = Number(url.searchParams.get("before"));
    const from = Math.max(0, before - 120);
    await route.fulfill({ json: {
      protocolVersion: 2,
      events: transcript.slice(from, before),
      from,
      floor: 0,
    } });
  });
  return requests;
}

async function loadEarlier(page: Page) {
  await page.getByTestId("session-load-earlier").evaluate((button) => {
    (button as HTMLButtonElement).click();
  });
}

test("#1301 bottom load: tail snapshot first paint", async ({ page }) => {
  await page.addInitScript(() => {
    const observer = new MutationObserver(() => {
      const state = window as unknown as { __firstAgentText?: string };
      if (state.__firstAgentText) return;
      const messages = document.querySelectorAll('[data-testid="session-message-agent"]');
      const last = messages.item(messages.length - 1);
      if (last?.textContent) state.__firstAgentText = last.textContent;
    });
    const start = () => {
      if (!document.documentElement) {
        requestAnimationFrame(start);
        return;
      }
      observer.observe(document.documentElement, {
        childList: true, subtree: true, characterData: true,
      });
    };
    start();
  });
  const requests = await routeHistory(page);
  const socketUrls = await openSession(page);
  expect(socketUrls.length).toBeGreaterThan(0);
  for (const url of socketUrls) {
    expect(new URL(url).searchParams.get("snapshotTail")).toBe("1");
  }
  expect(await page.evaluate(
    () => (window as unknown as { __firstAgentText?: string }).__firstAgentText,
  )).toContain("Answer 99.");
  await expect(page.getByTestId("session-message-user")).toHaveCount(TAIL_TURNS);
  await expect(page.getByTestId("session-message-user").first()).toHaveText("Question 60");
  await expect(page.getByText("Question 0", { exact: true })).toHaveCount(0);
  const geometry = await page.getByTestId("session-thread").evaluate((thread) => {
    const turns = thread.querySelectorAll(".session-turn");
    return {
      scrollTop: thread.scrollTop,
      gap: thread.getBoundingClientRect().bottom
        - turns.item(turns.length - 1).getBoundingClientRect().bottom,
    };
  });
  expect(geometry.scrollTop).toBe(0);
  expect(Math.abs(geometry.gap)).toBeLessThanOrEqual(1);
  expect(requests).toHaveLength(0);
});

test("#1304 a short first-load tail auto-fills without interaction", async ({ page }) => {
  const requests = await routeHistory(page);
  await openSession(page, false, true);

  const thread = page.getByTestId("session-thread");
  await expect.poll(() => thread.evaluate((node) => node.scrollHeight > node.clientHeight)).toBe(true);
  await expect(page.getByTestId("session-message-user").first()).not.toHaveText("Question 98");

  const geometry = await thread.evaluate((node) => {
    const messages = node.querySelectorAll('[data-testid="session-message-agent"]');
    return {
      scrollTop: node.scrollTop,
      last: messages.item(messages.length - 1).textContent,
      gap: node.getBoundingClientRect().bottom
        - messages.item(messages.length - 1).getBoundingClientRect().bottom,
    };
  });
  expect(Math.abs(geometry.scrollTop)).toBeLessThanOrEqual(LIVE_EDGE_PIN_BAND_PX);
  expect(geometry.last).toContain(`Answer ${TOTAL_TURNS - 1}`);
  expect(geometry.gap).toBeGreaterThanOrEqual(0);
  expect(requests.length).toBeGreaterThan(0);
});

test("#1301 bottom load: load earlier pulls older history and keeps the anchor", async ({ page }) => {
  const requests = await routeHistory(page);
  await openSession(page);
  const thread = page.getByTestId("session-thread");
  const anchor = page.getByTestId("session-message-user").filter({ hasText: /^Question 80$/ });
  await anchor.evaluate((node) => node.scrollIntoView({ block: "center" }));
  await expect(anchor).toBeInViewport();
  const before = {
    top: await anchor.evaluate((node) => node.getBoundingClientRect().top),
    scrollTop: await thread.evaluate((node) => node.scrollTop),
  };
  expect(before.scrollTop).toBeLessThan(0);
  expect(requests).toHaveLength(0);
  await loadEarlier(page);
  await expect(page.getByTestId("session-message-user").filter({ hasText: /^Question 59$/ })).toHaveCount(1);
  expect(requests).toHaveLength(1);
  expect(requests[0].searchParams.get("before")).toBe(String(TAIL_FROM));
  await expect.poll(async () => Math.abs(
    await anchor.evaluate((node) => node.getBoundingClientRect().top) - before.top,
  )).toBeLessThanOrEqual(1);
  expect(Math.abs(await thread.evaluate((node) => node.scrollTop) - before.scrollTop)).toBeLessThanOrEqual(1);
  const count = await page.getByTestId("session-message-user").count();
  expect(count).toBeGreaterThan(TAIL_TURNS);
  expect(count).toBeLessThanOrEqual(TAIL_TURNS + Math.ceil(HISTORY_REVEAL_BATCH / 2));
  await expect(page.getByTestId("session-load-earlier")).toBeAttached();
  for (let batch = 0; batch < 3 && await page.getByTestId("session-load-earlier").count(); batch += 1) {
    const previous = await page.getByTestId("session-message-user").count();
    await loadEarlier(page);
    await expect.poll(() => page.getByTestId("session-message-user").count()).toBeGreaterThan(previous);
  }
  await expect(page.getByTestId("session-message-user")).toHaveCount(TOTAL_TURNS);
  await expect(page.getByTestId("session-message-agent")).toHaveCount(TOTAL_TURNS);
  await expect(page.getByTestId("session-message-user").first()).toHaveText("Question 0");
  await expect(page.getByTestId("session-load-earlier")).toHaveCount(0);
  expect(requests).toHaveLength(1);
});

test("#1301 bottom load: failed earlier fetch is retryable", async ({ page }) => {
  const requests = await routeHistory(page, true);
  await openSession(page);
  const thread = page.getByTestId("session-thread");
  const messages = page.locator('[data-testid="session-message-user"], [data-testid="session-message-agent"]');
  const before = await messages.allTextContents();
  const scrollTop = await thread.evaluate((node) => node.scrollTop);
  await loadEarlier(page);
  await expect(page.getByTestId("session-load-earlier")).toHaveText("Couldn't load earlier messages — tap to retry");
  expect(requests).toHaveLength(1);
  expect(await messages.allTextContents()).toEqual(before);
  expect(await thread.evaluate((node) => node.scrollTop)).toBe(scrollTop);
  await loadEarlier(page);
  await expect(page.getByTestId("session-message-user").filter({ hasText: /^Question 59$/ })).toHaveCount(1);
  expect(requests).toHaveLength(2);
  expect(requests.map((url) => url.searchParams.get("before"))).toEqual([String(TAIL_FROM), String(TAIL_FROM)]);
  await expect(page.getByTestId("session-load-earlier")).not.toHaveText("Couldn't load earlier messages — tap to retry");
  await expect(page.getByTestId("session-message-agent").last()).toContainText("Answer 99.");
});

test("#1301 bottom load: legacy server without transcriptFrom", async ({ page }) => {
  const requests = await routeHistory(page);
  await openSession(page, true);
  await expect(page.getByTestId("session-message-user")).toHaveCount(40);
  await expect(page.getByTestId("session-message-user").first()).toHaveText("Question 0");
  await expect(page.getByTestId("session-message-agent").last()).toContainText("Answer 39.");
  await expect(page.getByTestId("session-load-earlier")).toHaveCount(0);
  await page.getByTestId("session-thread").evaluate((node) => {
    node.scrollTop = node.clientHeight - node.scrollHeight;
  });
  await expect(page.getByTestId("session-message-user").first()).toBeInViewport();
  await expect(page.getByTestId("session-load-earlier")).toHaveCount(0);
  expect(requests).toHaveLength(0);
});
