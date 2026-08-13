import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";

// Hard file-scope stub: late microtasks must never reach jsdom's real WebSocket.
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

describe("App new-task sheet route coupling", () => {
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

  it("does not show the new-task sheet after starting a task and navigating back to the dashboard", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/tasks") {
          return Promise.resolve(jsonResponse({ ok: true, cockpit }));
        }
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    await screen.findByText("Fix login");

    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(screen.getByTestId("new-task-sheet")).toBeInTheDocument();

    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Swipe back test" } });
    fireEvent.submit(screen.getByRole("form", { name: "New task" }));

    await screen.findByTestId("outlet-task");
    expect(screen.queryByTestId("new-task-sheet")).not.toBeInTheDocument();

    setHash("#/");
    await screen.findByTestId("outlet-dashboard");
    expect(screen.queryByTestId("new-task-sheet")).not.toBeInTheDocument();
  });

  it("closes the new-task sheet when opening a task route while the sheet is still open", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    await screen.findByText("Fix login");

    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(screen.getByTestId("new-task-sheet")).toBeInTheDocument();

    setHash("#/t/web%2Ffix-login");
    await screen.findByTestId("outlet-task");
    expect(screen.queryByTestId("new-task-sheet")).not.toBeInTheDocument();

    // Late reopen while still on the task (click-through onto New) must not
    // survive swipe-back to the dashboard.
    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(screen.queryByTestId("new-task-sheet")).not.toBeInTheDocument();

    setHash("#/");
    await screen.findByTestId("outlet-dashboard");
    expect(screen.queryByTestId("new-task-sheet")).not.toBeInTheDocument();
  });

  it("closes the new-task sheet when navigating to Settings", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") return Promise.resolve(jsonResponse({ ok: true }));
        if (path === "/api/health") return Promise.resolve(jsonResponse({ status: "ok" }));
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    await screen.findByText("Fix login");

    fireEvent.click(screen.getByRole("button", { name: "New" }));
    expect(screen.getByTestId("new-task-sheet")).toBeInTheDocument();

    setHash("#/settings");
    await screen.findByTestId("outlet-settings");
    expect(screen.queryByTestId("new-task-sheet")).not.toBeInTheDocument();
  });
});
