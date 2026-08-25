import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import TaskWorkspace from "./TaskWorkspace";
import taskDetail from "@/fixtures/task-detail.json";
import { writeOrchestrationChatEnabled } from "@/features/settings/public";
import { sessionHash, taskHash } from "@/shared/lib/routes";
import type { RemoteResource } from "@/shared/lib/types";
import type { BrowserTaskDetail, WebAction } from "@/shared/lib/types";
import * as api from "@/shared/lib/api";
import {
  readComposerDraft,
  readComposerQueue,
  writeComposerDraft,
  writeComposerQueue,
} from "@/features/chat/composer/public";
import { commitConfirmedAction } from "@/features/task/taskMutations";
import { DROP_UNDO_MS } from "@/shared/lib/polling";

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

function confirmFromShell(
  onResult: ReturnType<typeof vi.fn>,
  callbacks: Parameters<typeof commitConfirmedAction>[3],
  dropHandles: Parameters<typeof commitConfirmedAction>[4],
) {
  const options = onResult.mock.calls.at(-1)?.[3] as {
    pendingConfirm: { action: WebAction; handle: string; interactionId: string };
  };
  commitConfirmedAction(
    options.pendingConfirm.action,
    options.pendingConfirm.handle,
    options.pendingConfirm.interactionId,
    callbacks,
    dropHandles,
  );
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
    const header = screen.getByTestId("mobile-chrome-header");
    expect(header).toBeInTheDocument();
    expect(within(header).queryByText("Waiting", { exact: true })).not.toBeInTheDocument();
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

  describe("Drop dismiss composer cleanup", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
      vi.restoreAllMocks();
    });

    it("clears stored composer draft and queued follow-up when Drop dismisses", async () => {
      vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
      const onResult = vi.fn();
      const onDismiss = vi.fn();
      const dropHandles = {
        dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
        dropResolvedRef: { current: false },
      };
      writeComposerDraft("web/fix-login", "leftover draft");
      writeComposerQueue("web/fix-login", { status: "queued", text: "leftover queue" });
      render(
        <TaskWorkspace
          handle="web/fix-login"
          mode="terminal"
          detail={readyDetail(taskDetail)}
          orchestrationChat
          onGo={vi.fn()}
          onBack={vi.fn()}
          onOpenDiff={vi.fn()}
          onResult={onResult}
          onDismiss={onDismiss}
        />,
      );
      fireEvent.click(screen.getByText("Drop"));
      confirmFromShell(onResult, { onResult, onDismiss }, dropHandles);
      vi.advanceTimersByTime(DROP_UNDO_MS);
      await vi.runAllTimersAsync();
      expect(onDismiss).toHaveBeenCalledOnce();
      expect(readComposerDraft("web/fix-login")).toBe("");
      expect(readComposerQueue("web/fix-login")).toEqual({ status: "idle" });
    });

    it("does not clear composer presentation state when Drop is undone", async () => {
      vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
      const onResult = vi.fn();
      const dropHandles = {
        dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
        dropResolvedRef: { current: false },
      };
      writeComposerDraft("web/fix-login", "keep on undo");
      writeComposerQueue("web/fix-login", { status: "queued", text: "keep queue" });
      render(
        <TaskWorkspace
          handle="web/fix-login"
          mode="terminal"
          detail={readyDetail(taskDetail)}
          orchestrationChat
          onGo={vi.fn()}
          onBack={vi.fn()}
          onOpenDiff={vi.fn()}
          onResult={onResult}
        />,
      );
      fireEvent.click(screen.getByText("Drop"));
      confirmFromShell(onResult, { onResult }, dropHandles);
      const undoCall = onResult.mock.calls.find(
        (call) => call[0] === "Dropping web/fix-login…",
      )?.[3] as { onUndo: () => void };
      undoCall.onUndo();
      expect(readComposerDraft("web/fix-login")).toBe("keep on undo");
      expect(readComposerQueue("web/fix-login")).toEqual({
        status: "queued",
        text: "keep queue",
      });
    });
  });
});
