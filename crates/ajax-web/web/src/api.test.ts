import { describe, it, expect, vi, afterEach } from "vitest";
import {
  checkHealth,
  postAnswer,
  postOperation,
  restartServer,
  startTask,
  fetchCockpit,
  fetchDetail,
  fetchPane,
  fetchVersion,
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

const validPane = {
  sequence: 1,
  lines: ["ready"],
  truncated: false,
  tmux_exists: true,
  state: null,
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
      if (path === "/api/tasks/web%2Fx/pane?since=0") return Promise.resolve(json(validPane));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await fetchCockpit();
    await fetchVersion();
    await checkHealth();
    await fetchDetail("web/x");
    await fetchPane("web/x", 0);

    expect(fetch).toHaveBeenCalledWith("/api/cockpit", {
      cache: "no-store",
      credentials: "same-origin",
    });
    expect(fetch).toHaveBeenCalledWith("/api/version", {
      cache: "no-store",
      credentials: "same-origin",
    });
    expect(fetch).toHaveBeenCalledWith("/api/health", {
      cache: "no-store",
      credentials: "same-origin",
    });
    expect(fetch).toHaveBeenCalledWith("/api/tasks/web%2Fx", {
      cache: "no-store",
      credentials: "same-origin",
    });
    expect(fetch).toHaveBeenCalledWith("/api/tasks/web%2Fx/pane?since=0", {
      cache: "no-store",
      credentials: "same-origin",
    });
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
    });
    expect(fetch).toHaveBeenNthCalledWith(2, "/api/session", {
      method: "POST",
      cache: "no-store",
      credentials: "same-origin",
    });
    expect(fetch).toHaveBeenNthCalledWith(3, "/api/cockpit", {
      cache: "no-store",
      credentials: "same-origin",
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
    });
  });
});

describe("postAnswer status mapping", () => {
  it("maps 409 to a conflict error", async () => {
    mockFetch(() => json({ ok: false, error: "stale" }, 409));
    await expect(
      postAnswer("web/x", { answer: "approve", fingerprint: "f", request_id: "r" }),
    ).rejects.toMatchObject({ kind: "conflict" });
  });

  it("maps 422 to a terminal-escalation error", async () => {
    mockFetch(() => json({ ok: false, error: "terminal required" }, 422));
    await expect(
      postAnswer("web/x", { answer: "deny", fingerprint: "f", request_id: "r" }),
    ).rejects.toMatchObject({ kind: "terminal" });
  });

  it("maps 429 to a rate-limit error", async () => {
    mockFetch(() => json({ ok: false, error: "too many inputs" }, 429));
    await expect(
      postAnswer("web/x", { answer: "approve", fingerprint: "f", request_id: "r" }),
    ).rejects.toMatchObject({ kind: "rate-limit" });
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
    const answerRequest = {
      answer: "approve" as const,
      fingerprint: "fingerprint",
      request_id: "answer-request",
    };
    mockFetch((input) => {
      const path = String(input);
      if (path === "/api/operations") {
        return Promise.resolve(json({ ok: true, state_changed: false }));
      }
      if (path === "/api/tasks") {
        return Promise.resolve(json({ ok: true, state_changed: true, cockpit: validCockpit }));
      }
      if (path === "/api/tasks/web%2Fx/answer") {
        return Promise.resolve(json({ sequence_hint: 2 }));
      }
      if (path === "/api/server/restart") {
        return Promise.resolve(json({ ok: true, restarting: true }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await postOperation(operationRequest);
    await startTask(startRequest);
    await postAnswer("web/x", answerRequest);
    await restartServer();

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
    expect(fetch).toHaveBeenCalledWith("/api/tasks/web%2Fx/answer", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify(answerRequest),
    });
    expect(fetch).toHaveBeenCalledWith("/api/server/restart", {
      method: "POST",
      headers: { "content-type": "application/json" },
      cache: "no-store",
      credentials: "same-origin",
      body: JSON.stringify({}),
    });
  });
});
