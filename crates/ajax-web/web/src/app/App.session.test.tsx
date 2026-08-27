import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import App from "./App";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { writeOrchestrationChatEnabled } from "@/features/settings/public";
import { taskHash } from "@/shared/lib/routes";

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
        // Chat only renders for a task the host will attach.
        return Promise.resolve(jsonResponse({ ...taskDetail, session_capable: true }));
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

  it("redirects #/session/<handle> to the task when orchestration chat is off", async () => {
    writeOrchestrationChatEnabled(false);
    let wsConstructCount = 0;
    vi.stubGlobal(
      "WebSocket",
      class CountingWebSocket {
        readyState = 1;
        constructor() {
          wsConstructCount += 1;
        }
        close() {}
        addEventListener() {}
        send() {}
      },
    );
    stubFetch(true);
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session/web/fix-login");
    await waitFor(() => expect(window.location.hash).toBe(taskHash("web/fix-login")));
    expect(screen.queryByTestId("session-chat")).not.toBeInTheDocument();
    expect(wsConstructCount).toBe(0);
  });

  it("redirects #/session to the dashboard when orchestration chat is off", async () => {
    writeOrchestrationChatEnabled(false);
    stubFetch();
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session");
    await waitFor(() => expect(window.location.hash).toBe("#/"));
    expect(screen.getByTestId("outlet-dashboard")).toBeInTheDocument();
    expect(screen.queryByTestId("session-starter")).not.toBeInTheDocument();
  });

  it("opens the New Task sheet when orchestration chat is enabled (#911)", async () => {
    writeOrchestrationChatEnabled(true);
    stubFetch();
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session");
    expect(await screen.findByTestId("new-task-sheet")).toBeInTheDocument();
    expect(window.location.hash).toBe("#/session");
    expect(screen.queryByTestId("session-starter")).not.toBeInTheDocument();
  });

  // Found in dev: with chat on, every task opened as a session, but a task whose
  // agent still runs in tmux is refused by the host — the operator landed on a
  // dead socket instead of the terminal.
  it("sends a non-acp task that cannot hold a session back to the terminal", async () => {
    writeOrchestrationChatEnabled(true);
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/session/models")) {
          return Promise.resolve(jsonResponse({ models: [], default: "" }));
        }
        if (path.startsWith("/api/tasks/")) {
          return Promise.resolve(
            jsonResponse({ ...taskDetail, session_capable: false, agent: "Other" }),
          );
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session/web/fix-login");

    await waitFor(() => expect(window.location.hash).toBe(taskHash("web/fix-login")));
  });

  it("keeps acp-capable session URLs on chat when not yet provisioned (#1092)", async () => {
    writeOrchestrationChatEnabled(true);
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/session/models")) {
          return Promise.resolve(jsonResponse({ models: [{ id: "auto", label: "Auto" }] }));
        }
        if (path.startsWith("/api/tasks/")) {
          return Promise.resolve(
            jsonResponse({ ...taskDetail, session_capable: false, agent: "Codex" }),
          );
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session/web/fix-login");

    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    expect(window.location.hash).toBe("#/session/web/fix-login");
  });

  it("renders ChatSurface on #/session/<handle> when orchestration chat is enabled", async () => {
    writeOrchestrationChatEnabled(true);
    stubFetch(true);
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session/web/fix-login");
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    expect(await screen.findByTestId("session-head")).toBeInTheDocument();
    expect(screen.getByTestId("mobile-chrome-header")).toBeInTheDocument();
    expect(window.location.hash).toBe("#/session/web/fix-login");
    expect(screen.queryByTestId("outlet-task")).not.toBeInTheDocument();
  });
});
