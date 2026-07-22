import { describe, it, expect, vi, afterEach } from "vitest";
import {
  checkHealth,
  postOperation,
  restartServer,
  startTestInStable,
  startTask,
  fetchCockpit,
  fetchDetail,
  fetchVersion,
  openTaskTerminalSocket,
  taskTerminalWebSocketUrl,
} from "./api";

type FetchMock = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response> | Response;

function mockFetch(impl: FetchMock) {
  vi.stubGlobal("fetch", vi.fn(impl));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const validCockpit = {
  backend: { authority: "host-native", control_enabled: true },
  repos: { repos: [] },
  cards: [],
  inbox: { items: [] },
};

const validDetail = {
  qualified_handle: "web/x",
  repo: "web",
  title: "Fix x",
  branch: "ajax/x",
  base_branch: "main",
  worktree_path: "/repo/web__worktrees/ajax-x",
  tmux_session: "ajax-web-x",
  lifecycle: "Active",
  agent: "Codex",
  agent_status: "Waiting",
  status: "waiting",
  actions: [],
  annotations: [],
  created_unix_secs: 1700000000,
  last_activity_unix_secs: 1700000100,
  agent_attempts: [],
};

describe("fetchCockpit", () => {
  it("returns a validated cockpit on success", async () => {
    mockFetch(() => json(validCockpit));
    const cockpit = await fetchCockpit();
    expect(cockpit.cards).toEqual([]);
  });

  it("raises an incompatible-response error on malformed JSON shape", async () => {
    mockFetch(() => json({ nope: true }));
    await expect(fetchCockpit()).rejects.toMatchObject({ kind: "incompatible" });
  });

  it("raises a network error when fetch rejects", async () => {
    mockFetch(() => Promise.reject(new Error("offline")));
    await expect(fetchCockpit()).rejects.toMatchObject({ kind: "network" });
  });
});

describe("GET transport options", () => {
  it("sends same-origin credentials for protected Access routes", async () => {
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(json(validCockpit));
      if (path === "/api/version") return Promise.resolve(json({ version: "test" }));
      if (path === "/api/health") return Promise.resolve(json({ ok: true }));
      if (path === "/api/tasks/web%2Fx") return Promise.resolve(json(validDetail));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await fetchCockpit();
    await fetchVersion();
    await checkHealth();
    await fetchDetail("web/x");

    const getInit = { cache: "no-store", credentials: "same-origin", signal: expect.any(AbortSignal) };
    expect(fetch).toHaveBeenCalledWith("/api/cockpit", getInit);
    expect(fetch).toHaveBeenCalledWith("/api/version", getInit);
    expect(fetch).toHaveBeenCalledWith("/api/health", getInit);
    expect(fetch).toHaveBeenCalledWith("/api/tasks/web%2Fx", getInit);
  });
});

describe("GET request timeouts", () => {
  it("GET requests carry a per-call abort timeout signal", async () => {
    mockFetch(() => json(validCockpit));
    await fetchCockpit();
    const [, init] = vi.mocked(fetch).mock.calls[0];
    expect(init?.signal).toBeInstanceOf(AbortSignal);
    expect((init?.signal as AbortSignal).aborted).toBe(false);
  });

  it("session renewal POST carries an abort timeout signal", async () => {
    let cockpitCalls = 0;
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        cockpitCalls += 1;
        return Promise.resolve(
          cockpitCalls === 1
            ? json({ ok: false, error: "browser session required" }, 401)
            : json(validCockpit),
        );
      }
      if (path === "/api/session") return Promise.resolve(json({ ok: true }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await fetchCockpit();

    const sessionCall = vi
      .mocked(fetch)
      .mock.calls.find(([path]) => String(path) === "/api/session");
    const sessionInit = sessionCall?.[1];
    expect(sessionInit?.signal).toBeInstanceOf(AbortSignal);
  });

  it("mutation POSTs stay unbounded", async () => {
    mockFetch(() => json({ ok: true, state_changed: false }));
    await postOperation({
      task_handle: "web/x",
      action: "review",
      request_id: "r",
    });
    const [, init] = vi.mocked(fetch).mock.calls[0];
    expect(init?.signal).toBeUndefined();
  });

  it("each call gets a fresh, unaborted signal", async () => {
    mockFetch(() => json(validCockpit));
    await fetchCockpit();
    await fetchCockpit();
    const initA = vi.mocked(fetch).mock.calls[0][1];
    const initB = vi.mocked(fetch).mock.calls[1][1];
    const signalA = initA?.signal as AbortSignal;
    const signalB = initB?.signal as AbortSignal;
    expect(signalA).toBeInstanceOf(AbortSignal);
    expect(signalB).toBeInstanceOf(AbortSignal);
    expect(signalA).not.toBe(signalB);
    expect(signalA.aborted).toBe(false);
    expect(signalB.aborted).toBe(false);
  });
});

describe("browser session renewal", () => {
  it("renews the browser session once after a cockpit 401 and retries the GET", async () => {
    let cockpitCalls = 0;
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        cockpitCalls += 1;
        return Promise.resolve(
          cockpitCalls === 1
            ? json({ ok: false, error: "browser session required" }, 401)
            : json(validCockpit),
        );
      }
      if (path === "/api/session") return Promise.resolve(json({ ok: true }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    const cockpit = await fetchCockpit();

    expect(cockpit.cards).toEqual([]);
    expect(fetch).toHaveBeenNthCalledWith(1, "/api/cockpit", {
      cache: "no-store",
      credentials: "same-origin",
      signal: expect.any(AbortSignal),
    });
    expect(fetch).toHaveBeenNthCalledWith(2, "/api/session", {
      method: "POST",
      cache: "no-store",
      credentials: "same-origin",
      signal: expect.any(AbortSignal),
    });
    expect(fetch).toHaveBeenNthCalledWith(3, "/api/cockpit", {
      cache: "no-store",
      credentials: "same-origin",
      signal: expect.any(AbortSignal),
    });
  });

  it("renews the browser session once after a mutation 401 and retries the same POST", async () => {
    const operationRequest = {
      task_handle: "web/x",
      action: "review",
      request_id: "operate-request",
    };
    let operationCalls = 0;
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/operations") {
        operationCalls += 1;
        return Promise.resolve(
          operationCalls === 1
            ? json({ ok: false, state_changed: false, error: "browser session required" }, 401)
            : json({ ok: true, state_changed: true, cockpit: validCockpit, output: "done" }),
        );
      }
      if (path === "/api/session") return Promise.resolve(json({ ok: true }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    const result = await postOperation(operationRequest);

    expect(result.ok).toBe(true);
    expect(fetch).toHaveBeenNthCalledWith(1, "/api/operations", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify(operationRequest),
    });
    expect(fetch).toHaveBeenNthCalledWith(2, "/api/session", {
      method: "POST",
      cache: "no-store",
      credentials: "same-origin",
      signal: expect.any(AbortSignal),
    });
    expect(fetch).toHaveBeenNthCalledWith(3, "/api/operations", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify(operationRequest),
    });
  });

  it("surfaces stale-session when a retried mutation still returns 401", async () => {
    const operationRequest = {
      task_handle: "web/x",
      action: "review",
      request_id: "operate-request",
    };
    let operationCalls = 0;
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/operations") {
        operationCalls += 1;
        return Promise.resolve(json({ ok: false, error: "browser session required" }, 401));
      }
      if (path === "/api/session") return Promise.resolve(json({ ok: true }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await expect(postOperation(operationRequest)).rejects.toMatchObject({
      kind: "stale-session",
    });
    expect(operationCalls).toBe(2);
  });

  it("surfaces stale-session when renewal fails", async () => {
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        return Promise.resolve(json({ ok: false, error: "browser session required" }, 401));
      }
      if (path === "/api/session") {
        return Promise.resolve(json({ ok: false, error: "renew failed" }, 503));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await expect(fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
  });

  it("surfaces stale-session when renewal returns an unsuccessful JSON body", async () => {
    let cockpitCalls = 0;
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        cockpitCalls += 1;
        return Promise.resolve(json({ ok: false, error: "browser session required" }, 401));
      }
      if (path === "/api/session") {
        return Promise.resolve(json({ ok: false, error: "renew failed" }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await expect(fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(cockpitCalls).toBe(1);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("shares one session renewal across concurrent protected 401s", async () => {
    let cockpitCalls = 0;
    let detailCalls = 0;
    let releaseSession!: () => void;
    const sessionStarted = new Promise<void>((resolve) => {
      releaseSession = resolve;
    });
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/cockpit") {
        cockpitCalls += 1;
        return Promise.resolve(cockpitCalls === 1 ? json({ ok: false }, 401) : json(validCockpit));
      }
      if (path === "/api/tasks/web%2Fx") {
        detailCalls += 1;
        return Promise.resolve(detailCalls === 1 ? json({ ok: false }, 401) : json(validDetail));
      }
      if (path === "/api/session") {
        return sessionStarted.then(() => json({ ok: true }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    const cockpitPromise = fetchCockpit();
    const detailPromise = fetchDetail("web/x");
    await Promise.resolve();
    releaseSession();

    await expect(cockpitPromise).resolves.toMatchObject({ cards: [] });
    await expect(detailPromise).resolves.toMatchObject({ qualified_handle: "web/x" });
    const sessionCalls = vi
      .mocked(fetch)
      .mock.calls.filter(([path]) => String(path) === "/api/session");
    expect(sessionCalls).toHaveLength(1);
  });

  it("does not renew the browser session for health checks", async () => {
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/health") return Promise.resolve(json({ ok: false }, 401));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await expect(checkHealth()).resolves.toBe(false);
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(fetch).toHaveBeenCalledWith("/api/health", {
      cache: "no-store",
      credentials: "same-origin",
      signal: expect.any(AbortSignal),
    });
  });
});

describe("postOperation", () => {
  it("returns the refreshed cockpit projection on success", async () => {
    mockFetch(() =>
      json({ ok: true, state_changed: true, cockpit: validCockpit, output: "done" }),
    );
    const result = await postOperation({
      task_handle: "web/x",
      action: "review",
      request_id: "r",
    });
    expect(result.ok).toBe(true);
    expect(result.response.cockpit?.cards).toEqual([]);
  });

  it("rejects a non-JSON mutation response as incompatible", async () => {
    mockFetch(
      () => new Response("boom", { status: 500, headers: { "content-type": "text/plain" } }),
    );
    await expect(
      postOperation({
        task_handle: "web/x",
        action: "review",
        request_id: "r",
      }),
    ).rejects.toMatchObject({ kind: "incompatible" });
  });

  it("rejects a malformed mutation envelope as incompatible", async () => {
    mockFetch(() => json({ ok: true, cockpit: { cards: "not-an-array" } }));

    await expect(
      postOperation({
        task_handle: "web/x",
        action: "review",
        request_id: "r",
      }),
    ).rejects.toMatchObject({ kind: "incompatible" });
  });

  it("preserves server request_id and error detail on non-2xx responses", async () => {
    mockFetch(() =>
      json(
        {
          ok: false,
          request_id: "operate-request-409",
          error: "operation already in progress",
          state_changed: false,
        },
        409,
      ),
    );
    const result = await postOperation({
      task_handle: "web/x",
      action: "review",
      request_id: "operate-request-409",
    });
    expect(result.ok).toBe(false);
    expect(result.response.request_id).toBe("operate-request-409");
    expect(result.response.error).toBe("operation already in progress");
    expect(result.error?.message).toBe("operation already in progress");
    expect(result.error?.body?.request_id).toBe("operate-request-409");
    expect(result.error?.kind).toBe("conflict");
  });

  it("preserves server request_id and error detail when start-task conflicts", async () => {
    mockFetch(() =>
      json(
        {
          ok: false,
          request_id: "start-request-409",
          error: "task start already in progress",
        },
        409,
      ),
    );
    const result = await startTask({
      repo: "web",
      title: "Fix x",
      agent: "codex",
      request_id: "start-request-409",
    });
    expect(result.ok).toBe(false);
    expect(result.response.request_id).toBe("start-request-409");
    expect(result.response.error).toBe("task start already in progress");
    expect(result.error?.body?.request_id).toBe("start-request-409");
    expect(result.error?.message).toBe("task start already in progress");
  });
});

describe("POST transport options", () => {
  it("sends same-origin credentials for protected Access mutations", async () => {
    const operationRequest = {
      task_handle: "web/x",
      action: "review",
      request_id: "operate-request",
    };
    const startRequest = {
      repo: "web",
      title: "Fix x",
      agent: "codex",
      request_id: "start-request",
    };
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/operations") {
        return Promise.resolve(json({ ok: true, state_changed: false }));
      }
      if (path === "/api/tasks") {
        return Promise.resolve(json({ ok: true, state_changed: true, cockpit: validCockpit }));
      }
      if (path === "/api/server/restart") {
        return Promise.resolve(json({ ok: true, restarting: true }));
      }
      if (path === "/api/server/test-in-stable") {
        return Promise.resolve(json({ ok: true, restarting: true }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await postOperation(operationRequest);
    await startTask(startRequest);
    await restartServer();
    await startTestInStable();

    expect(fetch).toHaveBeenCalledWith("/api/operations", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify(operationRequest),
    });
    expect(fetch).toHaveBeenCalledWith("/api/tasks", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify(startRequest),
    });
    expect(fetch).toHaveBeenCalledWith("/api/server/restart", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify({}),
    });
    expect(fetch).toHaveBeenCalledWith("/api/server/test-in-stable", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify({}),
    });
  });
});

describe("startTestInStable", () => {
  it("posts to /api/server/test-in-stable", async () => {
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/server/test-in-stable") {
        return Promise.resolve(json({ ok: true, restarting: true }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    const response = await startTestInStable();

    expect(response.ok).toBe(true);
    expect(fetch).toHaveBeenCalledWith("/api/server/test-in-stable", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify({}),
    });
  });
});

describe("task terminal socket helpers", () => {
  it("builds an encoded terminal websocket URL", () => {
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        protocol: "https:",
        host: "ajax.local:8787",
      },
    });

    expect(taskTerminalWebSocketUrl("web/fix-login")).toBe(
      "wss://ajax.local:8787/api/tasks/web%2Ffix-login/terminal",
    );
  });

  it("uses ws for plain http pages", () => {
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        protocol: "http:",
        host: "localhost:8788",
      },
    });

    expect(taskTerminalWebSocketUrl("web/fix-login")).toBe(
      "ws://localhost:8788/api/tasks/web%2Ffix-login/terminal",
    );
  });

  it("opens a websocket at the task terminal URL", () => {
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        protocol: "https:",
        host: "ajax.local:8787",
      },
    });
    const socket = openTaskTerminalSocket("web/fix-login");
    expect(socket.url).toBe("wss://ajax.local:8787/api/tasks/web%2Ffix-login/terminal");
    socket.close();
  });
});
