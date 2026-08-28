import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import RuntimeControlView from "./RuntimeControlView";
import { useRuntimeControl } from "./useRuntimeControl";

const baseHook = {
  status: {
    ok: true,
    version: "0.11.0",
    commit: "abc123",
    profile: "stable",
    uptime_seconds: 90,
    update_available: { known: true, available: true },
    operation: { kind: "restart", phase: "succeeded", result: "succeeded" },
    logs: ["restarting stable"],
    test_in_stable: true,
  },
  loading: false,
  busy: false,
  overlay: null,
  error: null,
  dismissError: vi.fn(),
  confirmAction: null,
  updateAvailable: true,
  operationLabel: "restart · succeeded",
  terminalResult: "succeeded" as const,
  refresh: vi.fn(),
  runRestart: vi.fn(),
  runUpdate: vi.fn(),
};

vi.mock("./useRuntimeControl", () => ({
  useRuntimeControl: vi.fn(() => baseHook),
}));

describe("RuntimeControlView", () => {
  it("renders status, actions, and logs", () => {
    render(<RuntimeControlView />);
    expect(screen.getByTestId("runtime-control-status")).toBeInTheDocument();
    expect(screen.getByTestId("runtime-restart")).toBeInTheDocument();
    expect(screen.getByTestId("runtime-update")).toBeInTheDocument();
    expect(screen.getByText("restarting stable")).toBeInTheDocument();
  });

  it("shows a dismissible in-page error without the reconnect overlay", () => {
    const dismissError = vi.fn();
    vi.mocked(useRuntimeControl).mockReturnValueOnce({
      ...baseHook,
      error: "a runtime operation is already in progress",
      dismissError,
    });

    render(<RuntimeControlView />);
    expect(screen.getByTestId("runtime-control-error")).toBeInTheDocument();
    expect(
      screen.queryByText("Waiting for the listener to return…"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(dismissError).toHaveBeenCalled();
  });
});
