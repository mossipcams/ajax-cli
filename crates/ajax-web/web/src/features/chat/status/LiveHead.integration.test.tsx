import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { fireEvent, screen, act } from "@testing-library/react";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import {
  mountChat,
  prepareChatSurface,
  send,
  transport,
} from "../ChatSurface.testHarness";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
});

describe("LiveHead integration", () => {
  beforeEach(() => {
    prepareChatSurface();
  });

  it("shows ACP context usage in the head while idle and below 70%", () => {
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), status: "running" },
    });
    send({ type: "usage", used: 30, size: 100 });
    expect(screen.getByTestId("session-head")).toHaveAttribute("data-state", "idle");
    expect(screen.getByTestId("session-usage")).toHaveTextContent("Context 30% full");
  });

  it("does not render context usage for a zero-size update", () => {
    mountChat();
    send({ type: "usage", used: 0, size: 0 });
    expect(screen.queryByTestId("session-usage")).not.toBeInTheDocument();
  });

  it("stores per-turn token usage without showing it in the head", () => {
    mountChat();
    send({ type: "turn_usage", inputTokens: 1200, outputTokens: 300, totalTokens: 1500 });
    expect(screen.queryByTestId("session-turn-usage")).not.toBeInTheDocument();
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
    expect(screen.queryByTestId("session-decision")).not.toBeInTheDocument();
  });

  it("shows when a busy turn has stopped producing ACP activity", async () => {
    vi.useFakeTimers({
      toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
    });
    vi.setSystemTime(new Date("2026-08-15T12:00:00Z"));
    mountChat({
      detail: { ...(taskDetail as BrowserTaskDetail), status: "running" },
    });
    fireEvent.change(screen.getByLabelText("Message"), {
      target: { value: "Keep going" },
    });
    fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter" });

    expect(screen.getByTestId("session-head")).toHaveTextContent("Working");
    await act(async () => vi.advanceTimersByTimeAsync(60_000));
    expect(screen.getByTestId("session-head")).toHaveTextContent("No recent activity");
    vi.useRealTimers();
  });
});
