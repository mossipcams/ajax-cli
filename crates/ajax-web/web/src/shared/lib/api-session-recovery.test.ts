import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

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

describe("session-expiry recovery", () => {
  const locationDescriptor = Object.getOwnPropertyDescriptor(window, "location");
  let navigate: (url: string) => void;
  let api: typeof import("./api");

  beforeEach(() => {
    vi.resetModules();
    sessionStorage.clear();
    navigate = vi.fn();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: {
        href: "https://ajax.example/cockpit?x=1",
        replace: (url: string) => navigate(url),
      },
    });
  });

  afterEach(() => {
    if (locationDescriptor) Object.defineProperty(window, "location", locationDescriptor);
    vi.unstubAllGlobals();
  });

  const expiredFetch = () =>
    mockFetch((path) =>
      path === "/api/cockpit"
        ? Promise.resolve(new Response(null, { status: 401 }))
        : Promise.resolve(
            new Response(JSON.stringify({ error: "cloudflare access gone" }), {
              status: 503,
              headers: { "content-type": "application/json" },
            }),
          ),
    );

  it("navigates the top-level page to the current URL once when renewal fails", async () => {
    api = await import("./api");
    expiredFetch();

    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(navigate).toHaveBeenCalledTimes(1);
    expect(navigate).toHaveBeenCalledWith("https://ajax.example/cockpit?x=1");
  });

  it("does not navigate again while the recovery guard is already set", async () => {
    api = await import("./api");
    sessionStorage.setItem("ajax.pwa.session-expiry-recovered", "1");
    expiredFetch();

    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(navigate).not.toHaveBeenCalled();
  });

  it("navigates once when the retried request is still 401", async () => {
    api = await import("./api");
    mockFetch((path) => {
      if (path === "/api/session") {
        return Promise.resolve(
          new Response(JSON.stringify({ ok: true }), { status: 200 }),
        );
      }
      return Promise.resolve(new Response(null, { status: 401 }));
    });

    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(navigate).toHaveBeenCalledTimes(1);
  });

  it("keeps the guard after renewal alone and clears it after a protected success", async () => {
    const firstApi = await import("./api");
    expiredFetch();
    await expect(firstApi.fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(navigate).toHaveBeenCalledTimes(1);

    mockFetch(() => Promise.resolve(new Response(JSON.stringify({ ok: true }), { status: 200 })));
    await firstApi.renewBrowserSession();
    expect(sessionStorage.getItem("ajax.pwa.session-expiry-recovered")).toBe("1");

    expiredFetch();
    await expect(firstApi.fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(navigate).toHaveBeenCalledTimes(1);

    mockFetch(() => Promise.resolve(json(validCockpit)));
    await firstApi.fetchCockpit();
    expect(sessionStorage.getItem("ajax.pwa.session-expiry-recovered")).toBeNull();

    navigate = vi.fn();
    vi.resetModules();
    api = await import("./api");
    expiredFetch();
    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(navigate).toHaveBeenCalledTimes(1);
  });

  it("treats a generic protected fetch rejection as a retryable network failure without navigating", async () => {
    api = await import("./api");
    mockFetch(() => Promise.reject(new TypeError("load failed")));

    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "network" });
    expect(navigate).not.toHaveBeenCalled();
    expect(sessionStorage.getItem("ajax.pwa.session-expiry-recovered")).toBeNull();
  });

  it("recovers after a generic network outage without consuming the one-shot navigation", async () => {
    api = await import("./api");
    mockFetch(() => Promise.reject(new TypeError("load failed")));

    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "network" });
    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "network" });
    expect(navigate).not.toHaveBeenCalled();

    mockFetch(() => Promise.resolve(json(validCockpit)));
    const cockpit = await api.fetchCockpit();
    expect(cockpit.cards).toEqual([]);
    expect(navigate).not.toHaveBeenCalled();
  });

  it("recovers a login-page redirect via renewal and retry without navigating", async () => {
    api = await import("./api");
    let cockpitCalls = 0;
    mockFetch((path) => {
      if (path === "/api/session") return Promise.resolve(json({ ok: true }));
      cockpitCalls += 1;
      if (cockpitCalls === 1) {
        const loginPage = new Response("<html>cloudflare access</html>", {
          status: 200,
          headers: { "content-type": "text/html" },
        });
        Object.defineProperty(loginPage, "redirected", { value: true });
        return Promise.resolve(loginPage);
      }
      return Promise.resolve(json(validCockpit));
    });

    const cockpit = await api.fetchCockpit();

    expect(cockpit.cards).toEqual([]);
    expect(cockpitCalls).toBe(2);
    expect(navigate).not.toHaveBeenCalled();
  });

  it("clears the guard after successful reauthentication so a later expiry recovers again", async () => {
    api = await import("./api");
    expiredFetch();
    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(navigate).toHaveBeenCalledTimes(1);

    mockFetch(() => Promise.resolve(json(validCockpit)));
    await api.fetchCockpit();
    expect(sessionStorage.getItem("ajax.pwa.session-expiry-recovered")).toBeNull();

    expiredFetch();
    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "stale-session" });
    expect(navigate).toHaveBeenCalledTimes(2);
  });

  it("leaves the guard set when a protected request fails with a non-success status", async () => {
    api = await import("./api");
    sessionStorage.setItem("ajax.pwa.session-expiry-recovered", "1");
    mockFetch(() => Promise.resolve(new Response(null, { status: 500 })));

    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "http", status: 500 });
    expect(sessionStorage.getItem("ajax.pwa.session-expiry-recovered")).toBe("1");
  });

  it("does not navigate when a protected fetch is aborted", async () => {
    api = await import("./api");
    mockFetch(() => Promise.reject(new DOMException("aborted", "AbortError")));

    await expect(api.fetchCockpit()).rejects.toMatchObject({ kind: "network" });
    expect(navigate).not.toHaveBeenCalled();
  });

  it("renews and retries a version 401 during restart polling", async () => {
    api = await import("./api");
    vi.useFakeTimers();
    let versionCalls = 0;
    mockFetch((path) => {
      if (path === "/api/health") return Promise.resolve(json({ ok: true }));
      if (path === "/api/session") return Promise.resolve(json({ ok: true }));
      if (path === "/api/version") {
        versionCalls += 1;
        return Promise.resolve(
          versionCalls === 1 ? new Response(null, { status: 401 }) : json({ version: "2.0.0" }),
        );
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    try {
      const promise = api.waitForServerRestart({
        timeoutMs: 5000,
        pollMs: 500,
        previousVersion: "1.0.0",
      });
      await vi.advanceTimersByTimeAsync(1500);
      await expect(promise).resolves.toBe(true);
    } finally {
      vi.useRealTimers();
    }

    expect(versionCalls).toBe(2);
    expect(fetch).toHaveBeenCalledWith("/api/version", {
      cache: "no-store",
      credentials: "same-origin",
      headers: { "X-Requested-With": "XMLHttpRequest" },
      signal: expect.any(AbortSignal),
    });
    const sessionCalls = vi
      .mocked(fetch)
      .mock.calls.filter(([path]) => String(path) === "/api/session");
    expect(sessionCalls).toHaveLength(1);
    expect(navigate).not.toHaveBeenCalled();
  });

  it("navigates once when version renewal fails during restart polling", async () => {
    api = await import("./api");
    vi.useFakeTimers();
    mockFetch((path) => {
      if (path === "/api/health") return Promise.resolve(json({ ok: true }));
      if (path === "/api/version") return Promise.resolve(new Response(null, { status: 401 }));
      if (path === "/api/session") return Promise.resolve(json({ ok: false }, 503));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    try {
      const promise = api.waitForServerRestart({
        timeoutMs: 1000,
        pollMs: 200,
        previousVersion: "1.0.0",
      });
      await vi.advanceTimersByTimeAsync(1500);
      await expect(promise).resolves.toBe(false);
    } finally {
      vi.useRealTimers();
    }

    expect(navigate).toHaveBeenCalledTimes(1);
    expect(navigate).toHaveBeenCalledWith("https://ajax.example/cockpit?x=1");
  });

  it("sends the XMLHttpRequest marker on protected GET and POST requests", async () => {
    api = await import("./api");
    mockFetch((path) => {
      if (path === "/api/cockpit") return Promise.resolve(json(validCockpit));
      if (path === "/api/operations") {
        return Promise.resolve(json({ ok: true, state_changed: false }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });

    await api.fetchCockpit();
    await api.postOperation({ task_handle: "web/x", action: "review", request_id: "r" });

    const cockpitInit = vi.mocked(fetch).mock.calls[0][1];
    const operationInit = vi.mocked(fetch).mock.calls[1][1];
    expect(cockpitInit?.headers).toEqual(
      expect.objectContaining({ "X-Requested-With": "XMLHttpRequest" }),
    );
    expect(operationInit?.headers).toEqual(
      expect.objectContaining({ "X-Requested-With": "XMLHttpRequest" }),
    );
  });
});
