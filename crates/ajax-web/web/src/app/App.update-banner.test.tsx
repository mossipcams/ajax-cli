import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import { COCKPIT_RELOAD_PARAM } from "@/shared/lib/reloadCockpitDocument";

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
  });

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

  // #1007: production shell is GET `/` with a hash; replace(origin+hash) is a no-op.
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

  // #1007: post-#1008 health-gated reload looked like a dead tap when health failed.
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

    expect(replace).toHaveBeenCalledExactlyOnceWith(`https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=42#/`);
    expect(reload).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });
});
