import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { act } from "react";
import App from "./App";
import * as telemetry from "@/shared/lib/telemetry";
import { DROP_UNDO_MS } from "@/shared/lib/polling";
import { writeOrchestrationChatEnabled } from "@/features/session/sessionMode";
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

const sessionCapableDetail = { ...taskDetail, session_capable: true };

function stubSessionFetch(operations?: unknown[]) {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
      if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
      if (path.startsWith("/api/session/models")) {
        return Promise.resolve(jsonResponse({ models: [{ id: "auto", label: "Auto" }] }));
      }
      if (path.startsWith("/api/tasks/")) {
        return Promise.resolve(jsonResponse(sessionCapableDetail));
      }
      if (path === "/api/operations") {
        operations?.push(JSON.parse(String(init?.body ?? "{}")));
        return Promise.resolve(jsonResponse({ ok: true }));
      }
      return Promise.reject(new Error(`unexpected fetch: ${path}`));
    }),
  );
}

describe("App drop shell confirm", () => {
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

  it("cancels shell confirm after navigating away from the task", async () => {
    const completeSpy = vi.spyOn(telemetry, "endTapToOperationComplete");
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
    setHash("#/t/web%2Ffix-login");
    fireEvent.click(await screen.findByText("Drop"));
    expect(await screen.findByTestId("result-panel-confirm")).toBeInTheDocument();

    setHash("#/");
    expect(await screen.findByTestId("outlet-dashboard")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByTestId("result-panel-confirm")).not.toBeInTheDocument();
    });
    expect(completeSpy).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ ok: false, op: "drop", error_kind: "undo" }),
    );
  });

  it("cancels Drop confirmation when navigating to Settings", async () => {
    const operations: unknown[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/operations") {
          operations.push(JSON.parse(String(init?.body ?? "{}")));
          return Promise.resolve(jsonResponse({ ok: true }));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    fireEvent.click(await screen.findByText("Drop"));
    expect(await screen.findByTestId("result-panel-confirm")).toBeInTheDocument();

    setHash("#/settings");
    expect(await screen.findByTestId("outlet-settings")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByTestId("result-panel-confirm")).not.toBeInTheDocument();
    });
    expect(operations.some((operation) => (operation as { action?: string }).action === "drop")).toBe(
      false,
    );
  });

  it("keeps the switched-to task after Drop finishes for the previous task", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const operations: unknown[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/tasks/web%2Ffix-login") return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/tasks/web%2Fother")
          return Promise.resolve(
            jsonResponse({ ...taskDetail, qualified_handle: "web/other", title: "Other task" }),
          );
        if (path === "/api/operations") {
          operations.push(JSON.parse(String(init?.body ?? "{}")));
          return Promise.resolve(jsonResponse({ ok: true }));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    fireEvent.click(await screen.findByText("Drop"));
    fireEvent.click(await screen.findByText("Confirm"));
    expect(await screen.findByTestId("result-panel")).toHaveTextContent(/Dropping/);

    setHash("#/t/web%2Fother");
    expect(await screen.findByText("Other task")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);
    });

    await waitFor(() => {
      expect(operations.some((op) => (op as { action?: string }).action === "drop")).toBe(true);
    });
    expect(window.location.hash).toBe("#/t/web%2Fother");
    expect(screen.getByText("Other task")).toBeInTheDocument();
    expect(screen.queryByTestId("outlet-dashboard")).not.toBeInTheDocument();
  });

  it("cancels shell confirm before it can target another task", async () => {
    const operations: unknown[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/tasks/web%2Ffix-login") return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/tasks/web%2Fother")
          return Promise.resolve(
            jsonResponse({ ...taskDetail, qualified_handle: "web/other", title: "Other task" }),
          );
        if (path === "/api/operations") {
          operations.push(JSON.parse(String(init?.body ?? "{}")));
          return Promise.resolve(jsonResponse({ ok: true }));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    fireEvent.click(await screen.findByText("Drop"));
    expect(await screen.findByTestId("result-panel-confirm")).toBeInTheDocument();

    // Leave during shell confirm (before Confirm) — real phone path to another task.
    setHash("#/");
    expect(await screen.findByTestId("outlet-dashboard")).toBeInTheDocument();
    setHash("#/t/web%2Fother");
    expect(await screen.findByText("Other task")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByTestId("result-panel-confirm")).not.toBeInTheDocument();
    });
    expect(operations.some((op) => (op as { action?: string }).action === "drop")).toBe(false);
    expect(window.location.hash).toBe("#/t/web%2Fother");
    expect(screen.getByText("Other task")).toBeInTheDocument();
  });

  it("stays on the other task after Drop via dashboard intermediate", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const operations: unknown[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path === "/api/tasks/web%2Ffix-login") return Promise.resolve(jsonResponse(taskDetail));
        if (path === "/api/tasks/web%2Fother")
          return Promise.resolve(
            jsonResponse({ ...taskDetail, qualified_handle: "web/other", title: "Other task" }),
          );
        if (path === "/api/operations") {
          operations.push(JSON.parse(String(init?.body ?? "{}")));
          return Promise.resolve(jsonResponse({ ok: true }));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    fireEvent.click(await screen.findByText("Drop"));
    fireEvent.click(await screen.findByText("Confirm"));
    expect(await screen.findByTestId("result-panel")).toHaveTextContent(/Dropping/);

    setHash("#/");
    expect(await screen.findByTestId("outlet-dashboard")).toBeInTheDocument();
    setHash("#/t/web%2Fother");
    expect(await screen.findByText("Other task")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);
    });

    await waitFor(() => {
      expect(operations.some((op) => (op as { action?: string }).action === "drop")).toBe(true);
    });
    expect(window.location.hash).toBe("#/t/web%2Fother");
    expect(screen.getByText("Other task")).toBeInTheDocument();
    expect(screen.queryByTestId("outlet-dashboard")).not.toBeInTheDocument();
  });

  it("leaves the dropped task detail for the dashboard when Drop finishes in place", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
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
    setHash("#/t/web%2Ffix-login");
    fireEvent.click(await screen.findByText("Drop"));
    fireEvent.click(await screen.findByText("Confirm"));
    expect(await screen.findByTestId("result-panel")).toHaveTextContent(/Dropping/);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);
    });

    await waitFor(() => {
      expect(window.location.hash).toBe("#/");
    });
    expect(await screen.findByTestId("outlet-dashboard")).toBeInTheDocument();
  });

  it("does not POST Review while Drop confirm is open, and cancels the confirm", async () => {
    const operations: unknown[] = [];
    const detailWithReview = {
      ...taskDetail,
      actions: [
        {
          action: "review",
          label: "Review",
          destructive: false,
          confirmation_required: false,
        },
        ...taskDetail.actions,
      ],
    };
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/cockpit") return Promise.resolve(jsonResponse(cockpit));
        if (path === "/api/version") return Promise.resolve(jsonResponse({ version: "test" }));
        if (path.startsWith("/api/tasks/")) return Promise.resolve(jsonResponse(detailWithReview));
        if (path === "/api/operations") {
          operations.push(JSON.parse(String(init?.body ?? "{}")));
          return Promise.resolve(jsonResponse({ ok: true, cockpit }));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(<App />);
    setHash("#/t/web%2Ffix-login");
    fireEvent.click(await screen.findByRole("button", { name: /^Drop$/ }));
    expect(await screen.findByTestId("result-panel-confirm")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /^Review$/ }));
    await waitFor(() => {
      expect(screen.queryByTestId("result-panel-confirm")).not.toBeInTheDocument();
    });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 80));
    });

    expect(
      operations.filter((op) => (op as { action?: string }).action === "review"),
    ).toHaveLength(0);
    expect(
      operations.filter((op) => (op as { action?: string }).action === "drop"),
    ).toHaveLength(0);
  });
});

describe("App drop shell confirm on session chat (#947)", () => {
  beforeEach(() => {
    window.location.hash = "";
    localStorage.clear();
    writeOrchestrationChatEnabled(true);
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

  it("shows Confirm from session details, posts Drop, and dismisses to dashboard", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const operations: unknown[] = [];
    stubSessionFetch(operations);

    render(<App />);
    setHash("#/session/web/fix-login");
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    expect(await screen.findByTestId("session-task-panel")).toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: /^Drop$/ }));
    expect(await screen.findByTestId("result-panel-confirm")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByTestId("session-task-panel")).not.toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Confirm"));
    expect(await screen.findByTestId("result-panel")).toHaveTextContent(/Dropping/);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);
    });

    await waitFor(() => {
      expect(operations.some((op) => (op as { action?: string }).action === "drop")).toBe(true);
      expect(window.location.hash).toBe("#/");
    });
    expect(await screen.findByTestId("outlet-dashboard")).toBeInTheDocument();
  });

  it("keeps Drop confirm while staying on the same session handle", async () => {
    stubSessionFetch();

    render(<App />);
    setHash("#/session/web/fix-login");
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    fireEvent.click(await screen.findByRole("button", { name: /^Drop$/ }));
    expect(await screen.findByTestId("result-panel-confirm")).toBeInTheDocument();

    setHash("#/session/web/fix-login");
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("result-panel-confirm")).toBeInTheDocument();
  });

  it("cancels Drop confirm when navigating away from the session", async () => {
    const operations: unknown[] = [];
    stubSessionFetch(operations);

    render(<App />);
    setHash("#/session/web/fix-login");
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    fireEvent.click(await screen.findByRole("button", { name: /^Drop$/ }));
    expect(await screen.findByTestId("result-panel-confirm")).toBeInTheDocument();

    setHash("#/");
    expect(await screen.findByTestId("outlet-dashboard")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByTestId("result-panel-confirm")).not.toBeInTheDocument();
    });
    expect(operations.some((op) => (op as { action?: string }).action === "drop")).toBe(false);
  });
});
