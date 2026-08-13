import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { writeOrchestrationChatEnabled } from "@/features/session/sessionMode";

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

function stubFetch(includeTask = false) {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path === "/api/session/models") {
        return Promise.resolve(jsonResponse({ models: [{ id: "auto", label: "Auto" }] }));
      }
      if (includeTask && path.startsWith("/api/tasks/")) {
        return Promise.resolve(jsonResponse(taskDetail));
      }
      if (path.startsWith("/api/tasks/")) {
        return Promise.reject(new Error(`unexpected task fetch: ${path}`));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    }),
  );
}

describe("App session routing", () => {
  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
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
  });

  it("redirects #/session to the dashboard when orchestration chat is off", async () => {
    stubFetch();
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session");
    await waitFor(() => expect(window.location.hash).toBe("#/"));
    expect(screen.getByTestId("outlet-dashboard")).toBeInTheDocument();
    expect(screen.queryByTestId("session-starter")).not.toBeInTheDocument();
  });

  it("shows the session starter when orchestration chat is enabled", async () => {
    writeOrchestrationChatEnabled(true);
    stubFetch();
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session");
    expect(await screen.findByTestId("session-starter")).toBeInTheDocument();
    expect(window.location.hash).toBe("#/session");
  });

  it("renders SessionChat on #/session/<handle> when orchestration chat is enabled", async () => {
    writeOrchestrationChatEnabled(true);
    stubFetch(true);
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session/web/fix-login");
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    expect(await screen.findByTestId("session-head")).toBeInTheDocument();
    expect(window.location.hash).toBe("#/session/web/fix-login");
    expect(screen.queryByTestId("outlet-task")).not.toBeInTheDocument();
  });
});
