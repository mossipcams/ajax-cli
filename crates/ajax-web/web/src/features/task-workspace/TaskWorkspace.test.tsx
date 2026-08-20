import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import TaskWorkspace from "./TaskWorkspace";
import taskDetail from "@/fixtures/task-detail.json";
import { writeOrchestrationChatEnabled } from "@/features/settings/public";
import { sessionHash, taskHash } from "@/shared/lib/routes";
import type { RemoteResource } from "@/shared/lib/types";
import type { BrowserTaskDetail } from "@/shared/lib/types";

class StubWebSocket {
  readyState = 1;
  close() {}
  addEventListener() {}
  send() {}
}
globalThis.WebSocket = StubWebSocket as unknown as typeof WebSocket;

function readyDetail(data: BrowserTaskDetail): RemoteResource<BrowserTaskDetail> {
  return { status: "ready", data, error: null };
}

describe("TaskWorkspace", () => {
  beforeEach(() => {
    localStorage.clear();
    writeOrchestrationChatEnabled(true);
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

  it("redirects non-session-capable chat tasks to terminal", async () => {
    const onGo = vi.fn();
    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="chat"
        detail={readyDetail({ ...taskDetail, session_capable: false })}
        orchestrationChat
        onGo={onGo}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
      />,
    );

    await waitFor(() => expect(onGo).toHaveBeenCalledWith(taskHash("web/fix-login")));
  });

  it("renders terminal mode with task detail", () => {
    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="terminal"
        detail={readyDetail(taskDetail)}
        orchestrationChat
        onGo={vi.fn()}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
      />,
    );

    expect(screen.getByTestId("task-detail")).toBeInTheDocument();
  });

  it("renders chat mode for session-capable tasks", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path.startsWith("/api/session/models")) {
          return Promise.resolve({
            ok: true,
            status: 200,
            text: () => Promise.resolve(JSON.stringify({ models: [{ id: "auto", label: "Auto" }] })),
          });
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="chat"
        detail={readyDetail({ ...taskDetail, session_capable: true })}
        orchestrationChat
        onGo={vi.fn()}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
      />,
    );

    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("mobile-chrome-header")).toBeInTheDocument();
    expect(window.location.hash).not.toBe(sessionHash("web/fix-login"));
  });

  it("opens one task details sheet from chat header Details", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path.startsWith("/api/session/models")) {
          return Promise.resolve({
            ok: true,
            status: 200,
            text: () => Promise.resolve(JSON.stringify({ models: [{ id: "auto", label: "Auto" }] })),
          });
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="chat"
        detail={readyDetail({ ...taskDetail, session_capable: true })}
        orchestrationChat
        onGo={vi.fn()}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
      />,
    );

    await screen.findByTestId("session-chat");
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();
  });

  it("opens one task details sheet from terminal header Details", () => {
    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="terminal"
        detail={readyDetail({ ...taskDetail, session_capable: true })}
        orchestrationChat
        onGo={vi.fn()}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
      />,
    );

    expect(screen.getByTestId("task-detail")).toBeInTheDocument();
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("task-details"));
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();
  });

  it("opens one task details sheet from terminal footer Task details", () => {
    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="terminal"
        detail={readyDetail({ ...taskDetail, session_capable: true })}
        orchestrationChat
        onGo={vi.fn()}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
      />,
    );

    expect(screen.getByTestId("task-detail")).toBeInTheDocument();
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("task-meta-details-trigger"));
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();
  });
});
