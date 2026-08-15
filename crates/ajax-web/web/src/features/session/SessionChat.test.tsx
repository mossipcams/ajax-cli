import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent, screen, act } from "@testing-library/react";
import SessionChat, {
  formatSessionBrief,
  sessionSeededStorageKey,
} from "./SessionChat";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import { SWIPE_PAGE_COMMIT_MS } from "@/shared/hooks/useSwipePageTransition";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";

const transport = {
  sendPrompt: vi.fn(),
  sendCancel: vi.fn(),
  setModel: vi.fn(),
  respondPermission: vi.fn(),
  dispose: vi.fn(),
};

let emit: ((event: webSessionTransport.WebSessionServerEvent) => void) | undefined;
let ready: ((model: string) => void) | undefined;
let autoReady = true;

function stubSessionTransport() {
  vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
    (_handle, callbacks) => {
      emit = callbacks.onEvent;
      ready = callbacks.onReady;
      if (autoReady) callbacks.onReady("auto");
      return transport;
    },
  );
}

function mountChat(overrides: Partial<React.ComponentProps<typeof SessionChat>> = {}) {
  return render(
    <SessionChat
      handle="web/fix-login"
      detail={taskDetail as BrowserTaskDetail}
      detailStatus="ready"
      {...overrides}
    />,
  );
}

function send(event: webSessionTransport.WebSessionServerEvent) {
  act(() => emit?.(event));
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
});

describe("SessionChat smoke", () => {
  beforeEach(() => {
    emit = undefined;
    ready = undefined;
    autoReady = true;
    transport.sendPrompt.mockClear();
    transport.respondPermission.mockClear();
    localStorage.clear();
    sessionStorage.clear();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "auto", label: "Auto" },
            { id: "composer-2.5", label: "Composer 2.5" },
          ],
        }),
      }),
    );
    stubSessionTransport();
  });

  it("keeps replayed chat history when the session becomes ready", () => {
    autoReady = false;
    mountChat();
    send({ type: "message", role: "user", text: "Prior question" });
    send({ type: "message", role: "agent", text: "Prior answer" });

    act(() => ready?.("auto"));

    expect(screen.getByTestId("session-message-user")).toHaveTextContent("Prior question");
    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Prior answer");
  });

  it("leads with the live head", () => {
    mountChat();
    expect(screen.getByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("session-head")).toBeInTheDocument();
    expect(screen.getByTestId("session-composer")).toBeInTheDocument();
  });

  it("sends composer messages through ACP on Enter", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Please fix the flaky test" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("Please fix the flaky test");
    expect(screen.getByTestId("session-message-user")).toHaveTextContent(
      "Please fix the flaky test",
    );
    send({ type: "message", role: "user", text: "Please fix the flaky test" });
    expect(screen.getAllByTestId("session-message-user")).toHaveLength(1);
  });

  it("surfaces a permission request in the head with Approve and Reject", () => {
    mountChat();
    send({
      type: "permission_request",
      requestId: "7",
      title: "Run cargo test?",
      detail: "cargo test -p ajax-web",
    });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "decision");
    expect(screen.getByTestId("session-decision")).toHaveTextContent("Run cargo test?");
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(transport.respondPermission).toHaveBeenCalledWith("7", true);
  });

  it("seeds the session brief after transport is ready", () => {
    const starterContext = {
      title: "Fix the flaky test",
      constraints: "",
      expectedOutcome: "",
    };
    mountChat({ starterContext });
    const brief = formatSessionBrief(starterContext);
    expect(transport.sendPrompt).toHaveBeenCalledWith(brief);
    expect(sessionStorage.getItem(sessionSeededStorageKey("web/fix-login"))).toBe("1");
  });

  it("does not mark the session seeded when sendPrompt throws", () => {
    transport.sendPrompt.mockImplementation(() => {
      throw new Error("send failed");
    });
    mountChat({
      starterContext: {
        title: "Fix the flaky test",
        constraints: "",
        expectedOutcome: "",
      },
    });
    expect(sessionStorage.getItem(sessionSeededStorageKey("web/fix-login"))).toBeNull();
  });

  it("opens Diff Review on a left swipe", async () => {
    vi.useFakeTimers();
    const onOpenDiff = vi.fn();
    mountChat({ onOpenDiff });
    const root = screen.getByTestId("session-chat");
    Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });
    fireEvent.touchStart(root, { changedTouches: [{ clientX: 200, clientY: 40 }] });
    fireEvent.touchMove(root, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    fireEvent.touchEnd(root, { changedTouches: [{ clientX: 120, clientY: 42 }] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
    });
    expect(onOpenDiff).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });
});
