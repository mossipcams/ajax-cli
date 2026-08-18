import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import App from "./App";
import appSource from "./App.tsx?raw";
import cockpit from "@/fixtures/cockpit.json";
import taskDetail from "@/fixtures/task-detail.json";
import { writeOrchestrationChatEnabled } from "@/features/session/sessionMode";
import {
  readTaskTerminalPreferred,
  TASK_TERMINAL_PREFERENCE_STORAGE_KEY,
} from "@/features/session/taskViewPreference";
import { sessionHash, taskHash } from "@/shared/lib/routes";

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

const sessionCapableCockpit = {
  ...cockpit,
  cards: cockpit.cards.map((card) =>
    card.qualified_handle === "web/fix-login" ? { ...card, session_capable: true } : card,
  ),
};

function stubFetch() {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(sessionCapableCockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path.startsWith("/api/session/models")) {
        return Promise.resolve(jsonResponse({ models: [{ id: "auto", label: "Auto" }] }));
      }
      if (path.startsWith("/api/tasks/")) {
        return Promise.resolve(jsonResponse({ ...taskDetail, session_capable: true }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    }),
  );
}

describe("App task view preference", () => {
  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
    writeOrchestrationChatEnabled(true);
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
    localStorage.clear();
  });

  it("opens Ajax terminal from session task details, persists preference, and lands on #/t/", async () => {
    stubFetch();
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session/web/fix-login");
    await screen.findByTestId("session-chat");
    fireEvent.click(screen.getByTestId("session-details"));
    fireEvent.click(screen.getByTestId("session-ajax-terminal"));

    await waitFor(() => expect(window.location.hash).toBe(taskHash("web/fix-login")));
    expect(readTaskTerminalPreferred("web/fix-login")).toBe(true);
    expect(screen.getByTestId("outlet-task")).toBeInTheDocument();
    expect(screen.queryByTestId("session-terminal-sheet")).not.toBeInTheDocument();
  });

  it("opens terminal from the dashboard when the task prefers terminal view", async () => {
    localStorage.setItem(TASK_TERMINAL_PREFERENCE_STORAGE_KEY, JSON.stringify(["web/fix-login"]));
    stubFetch();
    render(<App />);
    await screen.findByText("Fix login");

    fireEvent.click(screen.getByText("Fix login"));

    await waitFor(() => expect(window.location.hash).toBe(taskHash("web/fix-login")));
    expect(screen.getByTestId("outlet-task")).toBeInTheDocument();
  });

  it("redirects #/session/<handle> to terminal while terminal preference is set", async () => {
    localStorage.setItem(TASK_TERMINAL_PREFERENCE_STORAGE_KEY, JSON.stringify(["web/fix-login"]));
    stubFetch();
    render(<App />);
    await screen.findByText("Fix login");

    setHash("#/session/web/fix-login");

    await waitFor(() => expect(window.location.hash).toBe(taskHash("web/fix-login")));
    expect(screen.getByTestId("outlet-task")).toBeInTheDocument();
  });

  it("returns to Ajax chat from the terminal task page and clears terminal preference", async () => {
    localStorage.setItem(TASK_TERMINAL_PREFERENCE_STORAGE_KEY, JSON.stringify(["web/fix-login"]));
    stubFetch();
    render(<App />);
    await screen.findByText("Fix login");

    setHash(taskHash("web/fix-login"));
    await screen.findByTestId("outlet-task");
    fireEvent.click(screen.getByTestId("task-details"));
    fireEvent.click(screen.getByRole("button", { name: "Ajax chat" }));

    await waitFor(() => expect(window.location.hash).toBe(sessionHash("web/fix-login")));
    expect(readTaskTerminalPreferred("web/fix-login")).toBe(false);
    expect(screen.getByTestId("session-chat")).toBeInTheDocument();
  });

  it("routes Diff Review back through terminal preference in App onBack", () => {
    const diffBlock = appSource.match(/<DiffReview[\s\S]*?\/>/)?.[0] ?? "";
    expect(diffBlock).toMatch(/readTaskTerminalPreferred\(route\.handle\)/);
    expect(diffBlock).toMatch(
      /orchestrationChat && sessionCapable && !terminalPreferred[\s\S]*?sessionHash\(route\.handle\)/,
    );
    expect(diffBlock).toMatch(/taskHash\(route\.handle\)/);
  });
});
