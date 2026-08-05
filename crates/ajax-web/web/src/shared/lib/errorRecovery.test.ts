import { describe, it, expect } from "vitest";
import { ApiError } from "./api";
import { operatorErrorPresentation } from "./errorRecovery";

describe("operatorErrorPresentation", () => {
  it("maps needs_terminal code with terminal suffix", () => {
    const result = operatorErrorPresentation({
      message: "Run this in tmux",
      code: "needs_terminal",
    });
    expect(result.hint).toBe("open_terminal");
    expect(result.telemetryKind).toBe("needs_terminal");
    expect(result.message).toBe("Run this in tmux — open the terminal");
  });

  it("does not duplicate terminal suffix when message already mentions terminal", () => {
    const result = operatorErrorPresentation({
      message: "Open the terminal to continue",
      code: "needs_terminal",
    });
    expect(result.message).toBe("Open the terminal to continue");
  });

  it("maps stale_session with reload suffix", () => {
    const result = operatorErrorPresentation({
      message: "browser session required",
      code: "stale_session",
    });
    expect(result.hint).toBe("reload_session");
    expect(result.telemetryKind).toBe("stale_session");
    expect(result.message).toBe("browser session required — reload the page");
  });

  it("falls back to stale-session kind", () => {
    const result = operatorErrorPresentation(
      new ApiError("stale-session", "Session expired", 401),
    );
    expect(result.telemetryKind).toBe("stale_session");
    expect(result.hint).toBe("reload_session");
  });

  it("maps conflict code and conflict kind", () => {
    expect(operatorErrorPresentation({ message: "busy", code: "conflict" }).telemetryKind).toBe(
      "conflict",
    );
    expect(operatorErrorPresentation(new ApiError("conflict", "busy", 409)).telemetryKind).toBe(
      "conflict",
    );
  });

  it("maps task_not_found", () => {
    const result = operatorErrorPresentation({ message: "missing", code: "task_not_found" });
    expect(result.hint).toBe("none");
    expect(result.telemetryKind).toBe("task_not_found");
    expect(result.message).toBe("missing");
  });

  it("maps confirmation_required", () => {
    const result = operatorErrorPresentation({
      message: "confirm first",
      code: "confirmation_required",
    });
    expect(result.hint).toBe("retry");
    expect(result.telemetryKind).toBe("confirmation_required");
  });

  it("maps unsupported and unknown actions to operation_failed", () => {
    expect(
      operatorErrorPresentation({ message: "nope", code: "unsupported_action" }).telemetryKind,
    ).toBe("operation_failed");
    expect(
      operatorErrorPresentation({ message: "nope", code: "unknown_action" }).telemetryKind,
    ).toBe("operation_failed");
  });

  it("maps command_failed and missing code to operation_failed with default message", () => {
    const coded = operatorErrorPresentation({ message: "", code: "command_failed" });
    expect(coded.message).toBe("Action failed");
    expect(coded.telemetryKind).toBe("operation_failed");

    const missing = operatorErrorPresentation({ message: "server said no" });
    expect(missing.message).toBe("server said no");
    expect(missing.telemetryKind).toBe("operation_failed");
  });

  it("maps network kind to network copy", () => {
    const result = operatorErrorPresentation({ kind: "network", message: "offline" });
    expect(result.message).toBe("Action failed — network error");
    expect(result.telemetryKind).toBe("network");
    expect(result.hint).toBe("retry");
  });

  it("reads ApiError code when present", () => {
    const error = new ApiError("conflict", "already running", 409, null, "conflict");
    expect(operatorErrorPresentation(error).telemetryKind).toBe("conflict");
  });
});
