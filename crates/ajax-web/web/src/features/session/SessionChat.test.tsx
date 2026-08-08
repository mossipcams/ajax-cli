import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, screen, waitFor, act } from "@testing-library/react";
import SessionStarter from "./SessionStarter";
import SessionChat from "./SessionChat";
import * as api from "@/shared/lib/api";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";

const repos = [{ name: "web" }];

const transport = {
  sendPrompt: vi.fn(),
  sendCancel: vi.fn(),
  respondPermission: vi.fn(),
  dispose: vi.fn(),
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe("SessionStarter", () => {
  it("submits cursor orchestration_chat startTask and reports handle with starter context", async () => {
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const onStarted = vi.fn();
    render(<SessionStarter repos={repos} onStarted={onStarted} />);
    expect(screen.getByTestId("session-agent-lock")).toHaveTextContent("Cursor");
    fireEvent.change(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    fireEvent.change(screen.getByLabelText("Constraints"), {
      target: { value: "No API changes" },
    });
    fireEvent.change(screen.getByLabelText("Expected outcome"), {
      target: { value: "Green tests" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Start session" }));
    await waitFor(() => expect(spy).toHaveBeenCalledOnce());
    expect(spy).toHaveBeenCalledWith(
      expect.objectContaining({
        agent: "cursor",
        orchestration_chat: true,
      }),
    );
    expect(onStarted).toHaveBeenCalledWith("web/fix-login", {
      title: "Fix login",
      constraints: "No API changes",
      expectedOutcome: "Green tests",
    });
  });
});

describe("SessionChat", () => {
  beforeEach(() => {
    transport.sendPrompt.mockClear();
    transport.sendCancel.mockClear();
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, callbacks) => {
        callbacks.onReady();
        return transport;
      },
    );
  });

  it("renders conversation thread with task details collapsed", () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    expect(screen.getByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("session-thread")).toBeInTheDocument();
    expect(screen.getByTestId("session-thread-empty")).toBeInTheDocument();
    expect(screen.queryByTestId("session-task-panel")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-artifact-status")).not.toBeInTheDocument();
    expect(screen.getByTestId("session-composer")).toBeInTheDocument();
    expect(screen.getByTestId("session-attention-banner")).toBeInTheDocument();
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("session-more"));
    expect(screen.getByTestId("session-task-panel")).toBeInTheDocument();
    expect(screen.getByTestId("session-artifact-status")).toHaveTextContent("Waiting for approval");
    expect(screen.getByTestId("session-artifact-activity")).toHaveTextContent("waiting for review");
    expect(screen.getByTestId("session-artifact-annotations")).toBeInTheDocument();
    expect(screen.getByTestId("session-quick-actions")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });

  it("sends composer messages through ACP transport", () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Please fix the flaky test" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Session composer" }));
    expect(transport.sendPrompt).toHaveBeenCalledWith("Please fix the flaky test");
    expect(screen.getByTestId("session-message-user")).toHaveTextContent(
      "Please fix the flaky test",
    );
  });

  it("sends one starter brief via ACP after ready", async () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
        starterContext={{
          title: "Fix login",
          constraints: "No API changes",
          expectedOutcome: "Green tests",
        }}
      />,
    );
    await waitFor(() => expect(transport.sendPrompt).toHaveBeenCalledOnce());
    expect(transport.sendPrompt).toHaveBeenCalledWith(
      "Task: Fix login\n\nConstraints:\nNo API changes\n\nExpected outcome:\nGreen tests",
    );
    expect(screen.getByTestId("session-message-user")).toHaveTextContent("Task: Fix login");
  });

  it("coalesces consecutive agent message chunks", () => {
    let onEvent: ((event: webSessionTransport.WebSessionServerEvent) => void) | undefined;
    vi.mocked(webSessionTransport.connectWebSessionTransport).mockImplementation(
      (_handle, callbacks) => {
        callbacks.onReady();
        onEvent = callbacks.onEvent;
        return transport;
      },
    );
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    act(() => {
      onEvent?.({ type: "message", role: "agent", text: "Hello " });
      onEvent?.({ type: "message", role: "agent", text: "world" });
    });
    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Hello world");
  });

  it("renders transport artifacts as structured cards", () => {
    let onEvent: ((event: webSessionTransport.WebSessionServerEvent) => void) | undefined;
    vi.mocked(webSessionTransport.connectWebSessionTransport).mockImplementation(
      (_handle, callbacks) => {
        callbacks.onReady();
        onEvent = callbacks.onEvent;
        return transport;
      },
    );
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    act(() => {
      onEvent?.({
        type: "artifact",
        kind: "plan",
        title: "Implementation plan",
        body: "Step one",
      });
    });
    const artifact = screen.getByTestId("session-transport-artifact-plan");
    expect(artifact).toHaveTextContent("Implementation plan");
    expect(artifact).toHaveTextContent("Step one");
    expect(screen.queryByText(/Artifact \(plan\):/)).not.toBeInTheDocument();
  });

  it("sends cancel when Stop is clicked", () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    fireEvent.click(screen.getByTestId("session-cancel"));
    expect(transport.sendCancel).toHaveBeenCalledOnce();
  });

  it("hides the attention banner when the task is running", () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={
          {
            ...(taskDetail as BrowserTaskDetail),
            status: "running",
            status_explanation: "Working",
          } as BrowserTaskDetail
        }
        detailStatus="ready"
      />,
    );
    expect(screen.queryByTestId("session-attention-banner")).not.toBeInTheDocument();
  });

  it("opens task details when the attention banner is clicked", () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    expect(screen.queryByTestId("session-task-panel")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-attention-banner"));
    expect(screen.getByTestId("session-task-panel")).toBeInTheDocument();
    expect(screen.getByTestId("session-artifact-status")).toHaveTextContent("Waiting for approval");
  });

  it("sends on Enter and keeps Shift+Enter as newline", () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    const message = screen.getByLabelText("Message");
    fireEvent.change(message, { target: { value: "Ship it" } });
    fireEvent.keyDown(message, { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("Ship it");
  });
});
