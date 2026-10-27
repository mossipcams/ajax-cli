import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import type { ComponentProps } from "react";
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

const chatSurfaceCapture = vi.hoisted(() => ({
  latest: null as null | { onSessionActivity?: (activity: { model: string; busy: boolean }) => void },
  stubSurface: false,
}));

vi.mock("@/features/chat/public", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/features/chat/public")>();
  const CapturingChatSurface = (props: ComponentProps<typeof actual.ChatSurface>) => {
    chatSurfaceCapture.latest = props;
    if (chatSurfaceCapture.stubSurface) return <section data-testid="session-chat" />;
    return <actual.ChatSurface {...props} />;
  };
  return { ...actual, ChatSurface: CapturingChatSurface };
});

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
    chatSurfaceCapture.stubSurface = false;
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

  it("keeps acp-capable chat tasks on chat when not yet provisioned (#1092)", async () => {
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

    const onGo = vi.fn();
    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="chat"
        detail={readyDetail({ ...taskDetail, session_capable: false, agent: "Codex" })}
        orchestrationChat
        onGo={onGo}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
      />,
    );

    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    expect(onGo).not.toHaveBeenCalled();
  });

  it("redirects non-acp chat tasks to terminal when session_capable is false", async () => {
    const onGo = vi.fn();
    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="chat"
        detail={readyDetail({ ...taskDetail, session_capable: false, agent: "Other" })}
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

  it("refreshes task detail only when the session stream busy state changes", async () => {
    chatSurfaceCapture.stubSurface = true;
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
    const onMutated = vi.fn();
    render(
      <TaskWorkspace
        handle="web/fix-login"
        mode="chat"
        detail={readyDetail({ ...taskDetail, session_capable: true })}
        orchestrationChat
        onGo={vi.fn()}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
        onMutated={onMutated}
      />,
    );
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    const emit = (busy: boolean) =>
      act(() => {
        chatSurfaceCapture.latest?.onSessionActivity?.({ model: "auto", busy });
      });

    emit(false);
    const baseline = onMutated.mock.calls.length;
    emit(true);
    expect(onMutated).toHaveBeenCalledTimes(baseline + 1);
    emit(true);
    expect(onMutated).toHaveBeenCalledTimes(baseline + 1);
    emit(false);
    expect(onMutated).toHaveBeenCalledTimes(baseline + 2);
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

  describe("session busy convergence", () => {
    beforeEach(() => {
      chatSurfaceCapture.stubSurface = true;
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
      chatSurfaceCapture.stubSurface = false;
    });

    function chatTree(status: BrowserTaskDetail["status"], onMutated: () => void) {
      return (
        <TaskWorkspace
          handle="web/fix-login"
          mode="chat"
          detail={readyDetail({ ...taskDetail, session_capable: true, status })}
          orchestrationChat
          onGo={vi.fn()}
          onBack={vi.fn()}
          onOpenDiff={vi.fn()}
          onMutated={onMutated}
        />
      );
    }

    const emit = (busy: boolean) =>
      act(() => {
        chatSurfaceCapture.latest?.onSessionActivity?.({ model: "auto", busy });
      });

    it("refetches every second while the stream is busy and the server status is not running", () => {
      const onMutated = vi.fn();
      render(chatTree("idle", onMutated));
      emit(false);
      emit(true);
      const afterFlip = onMutated.mock.calls.length;

      act(() => vi.advanceTimersByTime(1000));
      expect(onMutated).toHaveBeenCalledTimes(afterFlip + 1);
      act(() => vi.advanceTimersByTime(1000));
      expect(onMutated).toHaveBeenCalledTimes(afterFlip + 2);
    });

    it("stops interval refetches once the server status reports running", () => {
      const onMutated = vi.fn();
      const view = render(chatTree("idle", onMutated));
      emit(false);
      emit(true);
      act(() => vi.advanceTimersByTime(1000));

      view.rerender(chatTree("running", onMutated));
      const afterRunning = onMutated.mock.calls.length;
      act(() => vi.advanceTimersByTime(3000));
      expect(onMutated).toHaveBeenCalledTimes(afterRunning);
    });

    it.each(["waiting", "error"] as const)(
      "stops the converge refetch once the server status is %s",
      (status) => {
        const onMutated = vi.fn();
        render(chatTree(status, onMutated));
        emit(false);
        emit(true);
        const afterFlip = onMutated.mock.calls.length;

        act(() => vi.advanceTimersByTime(3000));
        expect(onMutated).toHaveBeenCalledTimes(afterFlip);
      },
    );

    it("does not interval-refetch when the stream is not busy", () => {
      const onMutated = vi.fn();
      render(chatTree("idle", onMutated));
      emit(false);
      emit(true);
      emit(false);
      const afterFlip = onMutated.mock.calls.length;

      act(() => vi.advanceTimersByTime(3000));
      expect(onMutated).toHaveBeenCalledTimes(afterFlip);
    });
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
