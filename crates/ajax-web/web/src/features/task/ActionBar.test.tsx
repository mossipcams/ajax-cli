import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import ActionBar from "./ActionBar";
import * as api from "@/shared/lib/api";
import * as telemetry from "@/shared/lib/telemetry";
import { commitConfirmedAction } from "./taskMutations";
import { DROP_UNDO_MS } from "@/shared/lib/polling";
import type { WebAction } from "@/shared/lib/types";

const review: WebAction = {
  action: "review",
  label: "Review",
  destructive: false,
  confirmation_required: false,
};
const drop: WebAction = {
  action: "drop",
  label: "Drop",
  destructive: true,
  confirmation_required: true,
};
const repair: WebAction = {
  action: "repair",
  label: "Repair",
  destructive: false,
  confirmation_required: true,
  branch_adoption: {
    expected_branch: "ajax/fix-login",
    observed_branch: "fix/pane-stuck",
  },
};

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

describe("ActionBar", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    localStorage.clear();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("renders only returned actions, first as primary", () => {
    render(<ActionBar actions={[review, drop]} handle="web/x" />);
    expect(screen.getByText("Review").classList.contains("primary")).toBe(true);
    expect(screen.getByText("Drop").classList.contains("primary")).toBe(false);
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("does not mark a sole Drop action as primary", () => {
    render(<ActionBar actions={[drop]} handle="web/x" />);
    const btn = screen.getByText("Drop");
    expect(btn.classList.contains("primary")).toBe(false);
    expect(btn.getAttribute("data-destructive")).toBe("true");
  });

  it("arms shell confirm on first destructive tap", () => {
    const onResult = vi.fn();
    render(<ActionBar actions={[drop]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Drop"));
    expect(onResult).toHaveBeenCalledWith(
      "Confirm Drop for web/x?",
      null,
      false,
      expect.objectContaining({
        pendingConfirm: expect.objectContaining({
          action: drop,
          handle: "web/x",
          interactionId: expect.any(String),
        }),
      }),
    );
  });

  it("commits destructive actions after shell confirm and undo window", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    render(<ActionBar actions={[drop]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Drop"));
    confirmFromShell(onResult, { onResult }, dropHandles);
    expect(spy).not.toHaveBeenCalled();
    vi.advanceTimersByTime(DROP_UNDO_MS);
    await vi.runAllTimersAsync();
    expect(spy).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ confirmed: true }),
    );
  });

  it("delays the Drop API until the undo window elapses, then dismisses", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const onDismiss = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    render(
      <ActionBar actions={[drop]} handle="web/x" onResult={onResult} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByText("Drop"));
    confirmFromShell(onResult, { onResult, onDismiss }, dropHandles);
    expect(spy).not.toHaveBeenCalled();
    expect(onResult).toHaveBeenCalledWith(
      "Dropping web/x…",
      null,
      false,
      expect.objectContaining({ onUndo: expect.any(Function), onCommit: expect.any(Function) }),
    );
    vi.advanceTimersByTime(DROP_UNDO_MS);
    await vi.runAllTimersAsync();
    expect(spy).toHaveBeenCalledOnce();
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it("cancels pending confirm on alternate action without posting", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onCancelPendingConfirm = vi.fn();
    render(
      <ActionBar
        actions={[review, drop]}
        handle="web/x"
        pendingConfirmAction="drop"
        onCancelPendingConfirm={onCancelPendingConfirm}
      />,
    );
    fireEvent.click(screen.getByText("Review"));
    expect(onCancelPendingConfirm).toHaveBeenCalledOnce();
    await vi.runAllTimersAsync();
    expect(spy).not.toHaveBeenCalled();
  });

  it("arms Drop confirm only once under same-turn double click", () => {
    const onResult = vi.fn();
    render(<ActionBar actions={[drop]} handle="web/x" onResult={onResult} />);
    const button = screen.getByText("Drop");
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(onResult).toHaveBeenCalledOnce();
  });

  it("posts immediate actions only once under same-turn double click", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    render(<ActionBar actions={[review]} handle="web/x" />);
    const button = screen.getByText("Review");
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.runAllTimersAsync();
    expect(spy).toHaveBeenCalledOnce();
  });

  it("keeps pending confirm when the same armed action is tapped again", () => {
    const onCancelPendingConfirm = vi.fn();
    const onResult = vi.fn();
    render(
      <ActionBar
        actions={[drop]}
        handle="web/x"
        onResult={onResult}
        pendingConfirmAction="drop"
        onCancelPendingConfirm={onCancelPendingConfirm}
      />,
    );
    fireEvent.click(screen.getByText("Drop"));
    expect(onCancelPendingConfirm).not.toHaveBeenCalled();
    expect(onResult).not.toHaveBeenCalled();
  });

  it("does not emit unmount telemetry when unmounting during shell confirm", () => {
    const completeSpy = vi.spyOn(telemetry, "endTapToOperationComplete");
    const onResult = vi.fn();
    const { unmount } = render(<ActionBar actions={[drop]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Drop"));
    unmount();
    expect(completeSpy).not.toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ error_kind: "unmount" }),
    );
  });

  it("does not emit unmount telemetry when unmounting during armed Drop undo", async () => {
    const completeSpy = vi.spyOn(telemetry, "endTapToOperationComplete");
    const onResult = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    const { unmount } = render(<ActionBar actions={[drop]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Drop"));
    confirmFromShell(onResult, { onResult }, dropHandles);
    unmount();
    expect(completeSpy).not.toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ error_kind: "unmount" }),
    );
  });

  it("completes Drop success telemetry after unmount when the undo window elapses", async () => {
    vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const completeSpy = vi.spyOn(telemetry, "endTapToOperationComplete");
    const onResult = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    const { unmount } = render(<ActionBar actions={[drop]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Drop"));
    confirmFromShell(onResult, { onResult }, dropHandles);
    unmount();
    vi.advanceTimersByTime(DROP_UNDO_MS);
    await vi.runAllTimersAsync();
    expect(completeSpy).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ ok: true, op: "drop" }),
    );
  });

  it("commits a pending Drop after unmount when the undo window elapses", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const onDismiss = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    const { unmount } = render(
      <ActionBar actions={[drop]} handle="web/x" onResult={onResult} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByText("Drop"));
    confirmFromShell(onResult, { onResult, onDismiss, isMounted: () => false }, dropHandles);
    unmount();
    vi.advanceTimersByTime(DROP_UNDO_MS);
    await vi.runAllTimersAsync();
    expect(spy).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ action: "drop", confirmed: true }),
    );
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("Undo cancels the pending Drop without calling the API", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const completeSpy = vi.spyOn(telemetry, "endTapToOperationComplete");
    const onResult = vi.fn();
    const onDismiss = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    render(
      <ActionBar actions={[drop]} handle="web/x" onResult={onResult} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByText("Drop"));
    confirmFromShell(onResult, { onResult, onDismiss }, dropHandles);
    const undoCall = onResult.mock.calls.find(
      (call) => call[0] === "Dropping web/x…",
    )?.[3] as { onUndo: () => void };
    undoCall.onUndo();
    expect(completeSpy).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ ok: false, op: "drop", error_kind: "undo" }),
    );
    vi.advanceTimersByTime(DROP_UNDO_MS);
    await vi.runAllTimersAsync();
    expect(spy).not.toHaveBeenCalled();
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("reuses the confirm interaction id through shell confirm", async () => {
    vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const beginSpy = vi.spyOn(telemetry, "beginInteraction");
    const onResult = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    render(<ActionBar actions={[drop]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Drop"));
    expect(beginSpy).toHaveBeenCalledTimes(1);
    confirmFromShell(onResult, { onResult }, dropHandles);
    expect(beginSpy).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(DROP_UNDO_MS);
    await vi.runAllTimersAsync();
  });

  it("routes to dismiss instead of refresh after a successful drop", async () => {
    vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onMutated = vi.fn();
    const onDismiss = vi.fn();
    const onResult = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    render(
      <ActionBar
        actions={[drop]}
        handle="web/x"
        onResult={onResult}
        onMutated={onMutated}
        onDismiss={onDismiss}
      />,
    );
    fireEvent.click(screen.getByText("Drop"));
    confirmFromShell(onResult, { onResult, onDismiss }, dropHandles);
    await vi.runAllTimersAsync();
    expect(onDismiss).toHaveBeenCalledOnce();
    expect(onMutated).not.toHaveBeenCalled();
  });

  it("routes to mutate instead of dismiss for non-destructive actions", async () => {
    vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onMutated = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ActionBar actions={[review]} handle="web/x" onMutated={onMutated} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByText("Review"));
    await vi.runAllTimersAsync();
    expect(onMutated).toHaveBeenCalledOnce();
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("runs a non-destructive action immediately and forwards the refreshed cockpit", async () => {
    const cockpit = {
      backend: { authority: "host-native", control_enabled: true },
      repos: { repos: [] },
      cards: [],
      inbox: { items: [] },
    };
    vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: { cockpit } });
    const onCockpit = vi.fn();
    render(<ActionBar actions={[review]} handle="web/x" onCockpit={onCockpit} />);
    fireEvent.click(screen.getByText("Review"));
    await vi.runAllTimersAsync();
    expect(onCockpit).toHaveBeenCalledWith(cockpit);
  });

  it("arms shell confirm for confirmation_required actions then runs on commit", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    render(<ActionBar actions={[repair]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Repair"));
    expect(spy).not.toHaveBeenCalled();
    confirmFromShell(onResult, { onResult }, dropHandles);
    await vi.runAllTimersAsync();
    expect(spy).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        task_handle: "web/x",
        action: "repair",
        confirmed: true,
        branch_adoption: {
          expected_branch: "ajax/fix-login",
          observed_branch: "fix/pane-stuck",
        },
        request_id: expect.any(String),
      }),
    );
  });

  it("retains the adoption pair from the first tap through shell confirm", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const dropHandles = {
      dropTimerRef: { current: null as ReturnType<typeof setTimeout> | null },
      dropResolvedRef: { current: false },
    };
    const refreshedRepair: WebAction = {
      ...repair,
      branch_adoption: {
        expected_branch: "ajax/fix-login",
        observed_branch: "fix/new-checkout",
      },
    };
    const { rerender } = render(<ActionBar actions={[repair]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Repair"));
    rerender(<ActionBar actions={[refreshedRepair]} handle="web/x" onResult={onResult} />);
    confirmFromShell(onResult, { onResult }, dropHandles);
    await vi.runAllTimersAsync();
    expect(spy).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        branch_adoption: {
          expected_branch: "ajax/fix-login",
          observed_branch: "fix/pane-stuck",
        },
      }),
    );
  });

  it("does not surface a completion toast on successful Review, even with output", async () => {
    vi.spyOn(api, "postOperation").mockResolvedValue({
      ok: true,
      response: { output: "Review passed" },
    });
    const onResult = vi.fn();
    render(<ActionBar actions={[review]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Review"));
    await vi.runAllTimersAsync();
    expect(onResult).not.toHaveBeenCalled();
  });

  it("marks ordinary actions unconfirmed and runs them immediately", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    render(<ActionBar actions={[review]} handle="web/x" />);
    fireEvent.click(screen.getByText("Review"));
    await vi.runAllTimersAsync();
    expect(spy).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        task_handle: "web/x",
        action: "review",
        confirmed: false,
        request_id: expect.any(String),
      }),
    );
    expect(spy.mock.calls[0][0]).not.toHaveProperty("branch_adoption");
  });

  it("surfaces coded operation failures through onResult", async () => {
    vi.spyOn(api, "postOperation").mockResolvedValue({
      ok: false,
      response: {
        ok: false,
        error: "Use tmux for this step",
        code: "needs_terminal",
        output: "stderr",
      },
      error: new api.ApiError("terminal", "Use tmux for this step", 422, null, "needs_terminal"),
    });
    const completeSpy = vi.spyOn(telemetry, "endTapToOperationComplete");
    const onResult = vi.fn();
    render(<ActionBar actions={[review]} handle="web/x" onResult={onResult} />);
    fireEvent.click(screen.getByText("Review"));
    await vi.runAllTimersAsync();
    expect(onResult).toHaveBeenCalledWith(
      "Use tmux for this step — open the terminal",
      "stderr",
      true,
    );
    expect(completeSpy).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({ ok: false, op: "review", error_kind: "needs_terminal" }),
    );
  });
});
