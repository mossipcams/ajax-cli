import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, render, screen } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { COCKPIT_RELOAD_PARAM, COCKPIT_RELOAD_WATCH_MS } from "@/shared/lib/reloadCockpitDocument";

class StubWebSocket {
  readyState = 1;
  close() {}
  addEventListener() {}
  send() {}
}
globalThis.WebSocket = StubWebSocket as unknown as typeof WebSocket;

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

describe("App update banner", () => {
  let loadedMeta: HTMLMetaElement | null = null;

  beforeEach(() => {
    window.location.hash = "";
    document.title = "";
    Object.defineProperty(document, "hidden", { configurable: true, value: false });
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      value: "visible",
    });
    vi.stubGlobal(
      "WebSocket",
      class {
        readyState = 1;
        close() {}
        addEventListener() {}
        send() {}
      },
    );
    vi.stubGlobal(
      "ResizeObserver",
      class MockResizeObserver {
        observe = vi.fn();
        disconnect = vi.fn();
      },
    );
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    loadedMeta?.remove();
    loadedMeta = null;
  });

  function loadedAppVersion(content: string) {
    loadedMeta = document.createElement("meta");
    loadedMeta.name = "ajax-app-version";
    loadedMeta.content = content;
    document.head.append(loadedMeta);
  }

  it("surfaces an update banner when the API version changes", async () => {
    vi.useFakeTimers();
    let versionCalls = 0;
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") {
        versionCalls += 1;
        return Promise.resolve(jsonResponse({ version: versionCalls === 1 ? "v1" : "v2" }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    const banner = screen.getByTestId("update-banner");

    expect(banner).not.toBeVisible();
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(versionCalls).toBe(1));
    expect(banner).not.toBeVisible();

    await vi.advanceTimersByTimeAsync(30000);

    await vi.waitFor(() => expect(banner).toBeVisible());
    expect(banner).toHaveTextContent("Update ready — tap to reload");
  });

  it("reloads only once when the update banner is multi-tapped (#1007)", async () => {
    vi.useFakeTimers();
    let versionCalls = 0;
    const replace = vi.fn();
    const reload = vi.fn();
    vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    vi.stubGlobal("location", {
      ...window.location,
      pathname: "/",
      hash: "#/",
      href: "https://ajax.local:8787/#/",
      origin: "https://ajax.local:8787",
      replace,
      reload,
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") {
        versionCalls += 1;
        return Promise.resolve(jsonResponse({ version: versionCalls === 1 ? "v1" : "v2" }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    const banner = screen.getByTestId("update-banner");
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(versionCalls).toBe(1));
    await vi.advanceTimersByTimeAsync(30000);
    await vi.waitFor(() => expect(banner).toBeVisible());

    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(replace).toHaveBeenCalledOnce());
    expect(replace).toHaveBeenCalledWith(
      `https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=1700000000000#/`,
    );
    expect(reload).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });

  it("navigates on banner tap without health or cockpit refetch (#1007)", async () => {
    vi.useFakeTimers();
    let versionCalls = 0;
    let postTapFetch = false;
    const replace = vi.fn();
    const reload = vi.fn();
    vi.spyOn(Date, "now").mockReturnValue(42);
    vi.stubGlobal("location", {
      ...window.location,
      pathname: "/",
      hash: "#/",
      href: "https://ajax.local:8787/#/",
      origin: "https://ajax.local:8787",
      replace,
      reload,
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (postTapFetch) {
        return Promise.reject(new Error(`unexpected post-tap fetch: ${path}`));
      }
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") {
        versionCalls += 1;
        return Promise.resolve(jsonResponse({ version: versionCalls === 1 ? "v1" : "v2" }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    const banner = screen.getByTestId("update-banner");
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(versionCalls).toBe(1));
    await vi.advanceTimersByTimeAsync(30000);
    await vi.waitFor(() => expect(banner).toBeVisible());

    postTapFetch = true;
    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    await vi.waitFor(() => expect(replace).toHaveBeenCalledOnce());
    expect(replace).toHaveBeenCalledExactlyOnceWith(`https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=42#/`);
    expect(reload).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });

  it("releases the reload latch when navigation is ignored so a later tap retries (#1007)", async () => {
    vi.useFakeTimers();
    let versionCalls = 0;
    let replaceCalls = 0;
    const replace = vi.fn(() => {
      replaceCalls += 1;
    });
    const reload = vi.fn();
    vi.spyOn(Date, "now").mockImplementation(() => 1_700_000_000_000 + replaceCalls);
    vi.stubGlobal("location", {
      ...window.location,
      pathname: "/",
      hash: "#/",
      href: "https://ajax.local:8787/#/",
      origin: "https://ajax.local:8787",
      replace,
      reload,
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") {
        versionCalls += 1;
        return Promise.resolve(jsonResponse({ version: versionCalls === 1 ? "v1" : "v2" }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    const banner = screen.getByTestId("update-banner");
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(versionCalls).toBe(1));
    await vi.advanceTimersByTimeAsync(30000);
    await vi.waitFor(() => expect(banner).toBeVisible());

    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(replace).toHaveBeenCalledOnce());

    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(replace).toHaveBeenCalledOnce();

    await vi.advanceTimersByTimeAsync(COCKPIT_RELOAD_WATCH_MS);
    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(replace).toHaveBeenCalledTimes(2));
    expect(replace).toHaveBeenLastCalledWith(
      `https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=1700000000001#/`,
    );
    vi.restoreAllMocks();
  });

  it("releases the reload latch immediately when location.replace throws (#1007)", async () => {
    vi.useFakeTimers();
    let versionCalls = 0;
    let replaceCalls = 0;
    const replace = vi.fn(() => {
      replaceCalls += 1;
      if (replaceCalls === 1) throw new Error("blocked");
    });
    const reload = vi.fn();
    vi.spyOn(Date, "now").mockImplementation(() => 40 + replaceCalls);
    vi.stubGlobal("location", {
      ...window.location,
      pathname: "/",
      hash: "#/",
      href: "https://ajax.local:8787/#/",
      origin: "https://ajax.local:8787",
      replace,
      reload,
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") {
        versionCalls += 1;
        return Promise.resolve(jsonResponse({ version: versionCalls === 1 ? "v1" : "v2" }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    const banner = screen.getByTestId("update-banner");
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(versionCalls).toBe(1));
    await vi.advanceTimersByTimeAsync(30000);
    await vi.waitFor(() => expect(banner).toBeVisible());

    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(replace).toHaveBeenCalledOnce());

    banner.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(replace).toHaveBeenCalledTimes(2));
    expect(replace).toHaveBeenLastCalledWith(
      `https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=41#/`,
    );
    vi.restoreAllMocks();
  });

  it("shows the update banner when the server is newer than the loaded page (#1679)", async () => {
    vi.useFakeTimers();
    loadedAppVersion("v1");
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "v2" }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    const banner = screen.getByTestId("update-banner");
    await vi.advanceTimersByTimeAsync(1);

    await vi.waitFor(() => expect(banner).toBeVisible());
    expect(banner).toHaveTextContent("Update ready — tap to reload");
  });

  it("reloads a stale chat page on resume when the server is newer (#1679)", async () => {
    vi.useFakeTimers();
    loadedAppVersion("v1");
    let serverVersion = "v1";
    const replace = vi.fn();
    vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    vi.stubGlobal("location", {
      ...window.location,
      pathname: "/",
      hash: "#/session/web%2Ffix-login",
      href: "https://ajax.local:8787/#/session/web%2Ffix-login",
      origin: "https://ajax.local:8787",
      replace,
      reload: vi.fn(),
    });
    const chatCockpit = {
      ...cockpit,
      cards: cockpit.cards.map((card) =>
        card.qualified_handle === "web/fix-login" ? { ...card, session_capable: true } : card,
      ),
    };
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(chatCockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: serverVersion }));
      if (path.startsWith("/api/tasks/web%2Ffix-login"))
        return Promise.resolve(jsonResponse({ ...taskDetail, session_capable: true }));
      if (path.startsWith("/api/session/models"))
        return Promise.resolve(jsonResponse({ models: [], default: "" }));
      if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    });
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(screen.getByTestId("session-chat")).toBeInTheDocument());

    serverVersion = "v2";
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
    await act(async () => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
    await act(async () => {
      document.dispatchEvent(new Event("visibilitychange"));
      await vi.advanceTimersByTimeAsync(1_000);
    });

    await vi.waitFor(() => expect(replace).toHaveBeenCalledOnce());
    expect(replace).toHaveBeenCalledWith(
      `https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=1700000000000#/session/web%2Ffix-login`,
    );
    vi.restoreAllMocks();
  });
});
