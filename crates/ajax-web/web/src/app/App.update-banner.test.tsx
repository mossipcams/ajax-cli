import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";

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

  it("reloads only once when the update banner is multi-tapped", async () => {
    vi.useFakeTimers();
    let versionCalls = 0;
    const replace = vi.fn();
    const reload = vi.fn();
    vi.stubGlobal("location", {
      ...window.location,
      hash: "",
      origin: "https://ajax.local:8787",
      replace,
      reload,
    });
    const fetchMock = vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/health") return Promise.resolve(jsonResponse({ ok: true }));
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
    await vi.waitFor(() =>
      expect(replace).toHaveBeenCalledOnce(),
    );
    expect(replace).toHaveBeenCalledWith("https://ajax.local:8787");
    expect(reload).not.toHaveBeenCalled();
  });
});
