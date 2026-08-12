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

function setHash(hash: string) {
  window.location.hash = hash;
  window.dispatchEvent(new HashChangeEvent("hashchange"));
}

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

describe("App shell skeletons", () => {
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
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    window.location.hash = "";
    document.title = "";
  });

  it("shows a dashboard skeleton while the cockpit projection is loading", () => {
    render(<App />);
    expect(screen.getByTestId("dashboard-skeleton")).toBeInTheDocument();
    expect(screen.queryByText(/All quiet|No tasks/)).not.toBeInTheDocument();
  });

  it("shows a task skeleton while a task detail is loading", async () => {
    const hang = new Promise(() => {});
    vi.stubGlobal("fetch", vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path.startsWith("/api/tasks/")) return hang;
      if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    }));
    render(<App />);
    setHash("#/t/web%2Ffix-login");
    await screen.findByTestId("outlet-task");
    expect(screen.getByTestId("task-skeleton")).toBeInTheDocument();
  });
});
