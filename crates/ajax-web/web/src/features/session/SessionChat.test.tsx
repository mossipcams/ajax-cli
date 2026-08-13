import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { sessionSeededStorageKey } from "./SessionChat";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import {
  transport,
  stubSessionTransport,
  mountChat,
  send,
} from "./SessionChat.test-helpers";

describe("SessionChat", () => {
  beforeEach(() => {
    stubSessionTransport();
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
  });

  it("leads with the live head and keeps task detail one tap away", () => {
    mountChat();
    expect(screen.getByTestId("session-chat")).toBeInTheDocument();
    expect(screen.getByTestId("session-head")).toBeInTheDocument();
    expect(screen.getByTestId("session-thread-empty")).toHaveTextContent(
      "Message the agent to steer this task.",
    );
    expect(screen.getByTestId("session-composer")).toBeInTheDocument();
    expect(screen.queryByTestId("session-task-panel")).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("session-details"));
    expect(screen.getByTestId("session-task-panel")).toBeInTheDocument();
    expect(screen.getByTestId("session-artifact-status")).toHaveTextContent("Waiting for approval");
    expect(screen.getByTestId("session-artifact-activity")).toHaveTextContent("waiting for review");
    expect(screen.getByTestId("session-artifact-annotations")).toBeInTheDocument();
    expect(screen.getByTestId("session-quick-actions")).toBeInTheDocument();
  });

  it("shows a task that needs attention in the head without a separate banner", () => {
    mountChat();
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "attention");
    expect(screen.getByTestId("session-attention")).toHaveTextContent("Waiting for approval");
  });

  it("reports idle when the task is neither waiting nor in error", () => {
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), status: "running" } as BrowserTaskDetail,
    });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "idle");
  });

  it("sends composer messages through ACP and echoes them in the transcript", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Please fix the flaky test" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Session composer" }));
    expect(transport.sendPrompt).toHaveBeenCalledWith("Please fix the flaky test");
    // The host records the prompt in the shared transcript and streams it back.
    send({ type: "message", role: "user", text: "Please fix the flaky test" });
    expect(screen.getByTestId("session-message-user")).toHaveTextContent(
      "Please fix the flaky test",
    );
  });

  it("sends on Enter and keeps Shift+Enter as newline", () => {
    mountChat();
    const message = screen.getByLabelText("Message");
    fireEvent.change(message, { target: { value: "Ship it" } });
    fireEvent.keyDown(message, { key: "Enter", shiftKey: true });
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    fireEvent.keyDown(message, { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("Ship it");
  });

  it("sends one starter brief via ACP after ready", async () => {
    mountChat({
      starterContext: {
        title: "Fix login",
        constraints: "No API changes",
        expectedOutcome: "Green tests",
      },
    });
    await waitFor(() => expect(transport.sendPrompt).toHaveBeenCalledOnce());
    expect(transport.sendPrompt).toHaveBeenCalledWith(
      "Fix login\n\nConstraints: No API changes\n\nDone when: Green tests",
    );
    expect(sessionStorage.getItem(sessionSeededStorageKey("web/fix-login"))).toBe("1");
    send({ type: "message", role: "user", text: "Fix login" });
    expect(screen.getByTestId("session-message-user")).toHaveTextContent("Fix login");
  });

  it("does not re-send the starter brief after remount with the same handle", async () => {
    const starterContext = {
      title: "Fix login",
      constraints: "No API changes",
      expectedOutcome: "Green tests",
    };
    const { unmount } = mountChat({ starterContext });
    await waitFor(() => expect(transport.sendPrompt).toHaveBeenCalledOnce());
    unmount();
    mountChat({ starterContext });
    expect(await screen.findByTestId("session-chat")).toBeInTheDocument();
    expect(transport.sendPrompt).toHaveBeenCalledOnce();
  });

  it("shows queued-send placeholder while a turn is in flight", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    expect(screen.getByLabelText("Message")).toHaveAttribute(
      "placeholder",
      "Sends after this turn…",
    );
  });

  it("shows stop-and-send placeholder after queuing a follow-up during a turn", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Ship the fix next" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).toHaveBeenCalledWith("Ship the fix next");
    expect(screen.getByLabelText("Message")).toHaveAttribute(
      "placeholder",
      "Enter again to stop and send",
    );
  });

  it("empty Enter after a queued follow-up cancels in flight but keeps the queue", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Ship the fix next" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    transport.sendCancel.mockClear();
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendCancel).toHaveBeenCalledWith(true);
    expect(transport.sendPrompt).toHaveBeenCalledWith("Ship the fix next");
  });

  it("double Enter while busy queues once then stops on the second press", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    const message = screen.getByLabelText("Message");
    fireEvent.change(message, { target: { value: "Ship the fix next" } });
    fireEvent.keyDown(message, { key: "Enter", shiftKey: false });
    transport.sendPrompt.mockClear();
    transport.sendCancel.mockClear();
    fireEvent.keyDown(message, { key: "Enter", shiftKey: false });
    expect(transport.sendPrompt).not.toHaveBeenCalled();
    expect(transport.sendCancel).toHaveBeenCalledWith(true);
  });

  it("idle send then empty Enter does not cancel", () => {
    mountChat();
    fireEvent.change(screen.getByLabelText("Message"), { target: { value: "Ship it" } });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    transport.sendCancel.mockClear();
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    expect(transport.sendCancel).not.toHaveBeenCalled();
  });

  it("Stop clears the queue with a plain cancel", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Ship the fix next" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
    transport.sendCancel.mockClear();
    fireEvent.click(screen.getByTestId("session-cancel"));
    expect(transport.sendCancel).toHaveBeenCalledWith();
  });

  it("renders a tool call as a labelled row in the head, not a JSON dump", () => {
    mountChat();
    send({
      type: "tool_call",
      callId: "c1",
      title: "Read configuration",
      kind: "read",
      status: "in_progress",
      locations: ["/repo/crates/ajax-web/src/lib.rs"],
    });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "working");
    const tool = screen.getByTestId("session-head-tool");
    expect(tool).toHaveTextContent("read");
    expect(tool).toHaveTextContent("Read configuration");
    expect(tool).toHaveTextContent("…/src/lib.rs");
    expect(screen.queryByTestId("session-tools")).not.toBeInTheDocument();
    expect(screen.queryByText(/sessionUpdate/)).not.toBeInTheDocument();
  });

  it("summarizes a turn's tools into one transcript note", () => {
    mountChat();
    send({
      type: "tool_call",
      callId: "c1",
      title: "Read configuration",
      kind: "read",
      status: "completed",
      locations: ["/repo/a.ts"],
    });
    send({
      type: "tool_call",
      callId: "c2",
      title: "Edit web_session.rs",
      kind: "edit",
      status: "completed",
      locations: ["/repo/b.ts"],
    });
    expect(screen.queryByTestId("session-tools")).not.toBeInTheDocument();
    send({ type: "turn_end", stopReason: "end_turn" });
    expect(screen.getByTestId("session-note-info")).toHaveTextContent("1 read · 1 edit");
  });

  it("shows only the in-progress plan step in the head", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    send({
      type: "plan",
      entries: [
        { content: "Read", status: "completed" },
        { content: "Patch the router", status: "in_progress" },
        { content: "Cover both orders", status: "pending" },
      ],
    });
    expect(screen.getByTestId("session-plan-step")).toHaveTextContent("Patch the router");
    expect(screen.queryByTestId("session-plan")).not.toBeInTheDocument();
    expect(screen.queryByText("Cover both orders")).not.toBeInTheDocument();
  });

  it("renders agent markdown as real code and list elements", () => {
    mountChat();
    send({
      type: "message",
      role: "agent",
      text: "Fixed it:\n\n- ran `cargo test`\n\n```\nok\n```",
    });
    send({ type: "turn_end", stopReason: "end_turn" });
    expect(screen.getByRole("listitem")).toHaveTextContent("ran cargo test");
    expect(screen.getByText("cargo test").tagName).toBe("CODE");
    expect(screen.getByText("ok").tagName).toBe("CODE");
  });

  it("surfaces a permission request as the head decision with Approve and Reject", () => {
    mountChat();
    send({
      type: "permission_request",
      requestId: "42",
      title: "Run cargo test?",
      detail: "in crates/ajax-web",
    });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "decision");
    const decision = screen.getByTestId("session-decision");
    expect(decision).toHaveTextContent("Run cargo test?");
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(transport.respondPermission).toHaveBeenCalledWith("42", true);
    expect(screen.queryByTestId("session-decision")).not.toBeInTheDocument();
  });

  it("rejects a permission request without approving it", () => {
    mountChat();
    send({ type: "permission_request", requestId: "42", title: "Delete the branch?" });
    fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    expect(transport.respondPermission).toHaveBeenCalledWith("42", false);
  });

  it("offers Stop only while a turn is in flight", () => {
    mountChat();
    expect(screen.queryByTestId("session-cancel")).not.toBeInTheDocument();
    send({ type: "message", role: "agent", text: "working" });
    fireEvent.click(screen.getByTestId("session-cancel"));
    expect(transport.sendCancel).toHaveBeenCalledOnce();
    send({ type: "turn_end", stopReason: "end_turn" });
    expect(screen.queryByTestId("session-cancel")).not.toBeInTheDocument();
  });

  it("keeps reasoning out of the transcript and the head", () => {
    mountChat();
    send({ type: "message", role: "thought", text: "Checking the router" });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "working");
    expect(screen.queryByTestId("session-thought")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-message-agent")).not.toBeInTheDocument();
    send({ type: "message", role: "agent", text: "Found it" });
    expect(screen.getByTestId("session-message-agent")).toHaveTextContent("Found it");
  });

  it("keeps run status out of the transcript", () => {
    mountChat();
    send({ type: "status", state: "running" });
    send({ type: "status", state: "running" });
    expect(screen.getByTestId("session-thread-empty")).toBeInTheDocument();
  });

  it("records a transport error in the transcript and ends the turn", () => {
    mountChat();
    send({ type: "message", role: "agent", text: "working" });
    send({ type: "error", message: "ACP process exited" });
    expect(screen.getByTestId("session-note-error")).toHaveTextContent(
      "The agent stopped. It will restart when you reconnect.",
    );
    expect(screen.queryByTestId("session-cancel")).not.toBeInTheDocument();
  });
});
