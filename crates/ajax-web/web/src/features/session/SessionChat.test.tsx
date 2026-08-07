import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, screen, waitFor } from "@testing-library/react";
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
      constraints: "No API changes",
      expectedOutcome: "Green tests",
    });
  });
});

describe("SessionChat", () => {
  beforeEach(() => {
    vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
      (_handle, callbacks) => {
        callbacks.onReady();
        return transport;
      },
    );
  });

  it("renders orchestration artifacts and composer", () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    expect(screen.getByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("session-artifact-status")).toHaveTextContent("Waiting for approval");
    expect(screen.getByTestId("session-artifact-activity")).toHaveTextContent("waiting for review");
    expect(screen.getByTestId("session-artifact-annotations")).toBeInTheDocument();
    expect(screen.getByTestId("session-quick-actions")).toBeInTheDocument();
    expect(screen.getByTestId("session-composer")).toBeInTheDocument();
    expect(screen.getByTestId("session-attention-banner")).toBeInTheDocument();
    expect(webSessionTransport.connectWebSessionTransport).toHaveBeenCalled();
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

  it("seeds starter constraints via ACP after ready", async () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
        starterContext={{ constraints: "No API changes", expectedOutcome: "Green tests" }}
      />,
    );
    await waitFor(() =>
      expect(transport.sendPrompt).toHaveBeenCalledWith("Constraints: No API changes"),
    );
    expect(transport.sendPrompt).toHaveBeenCalledWith("Expected outcome: Green tests");
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

  it("fills Retry / Try another approach into the composer without sending", () => {
    render(
      <SessionChat
        handle="web/fix-login"
        detail={taskDetail as BrowserTaskDetail}
        detailStatus="ready"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(screen.getByLabelText("Message")).toHaveValue("Retry the last step");
    fireEvent.click(screen.getByRole("button", { name: "Try another approach" }));
    expect(screen.getByLabelText("Message")).toHaveValue("Try another approach");
  });
});
