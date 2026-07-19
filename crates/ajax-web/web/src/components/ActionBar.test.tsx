import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen, act } from "@testing-library/react";
import ActionBar from "./ActionBar";
import * as api from "../api";
import { DROP_UNDO_MS } from "../polling";
import type { WebAction } from "../types";

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

describe("ActionBar", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("renders only returned actions, first as primary", () => {
    render(<ActionBar actions={[review, drop]} handle="web/x" />);
    expect(screen.getByText("Review").classList.contains("primary")).toBe(true);
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("requires two taps for a destructive action then commits after the undo window", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    render(<ActionBar actions={[drop]} handle="web/x" />);
    fireEvent.click(screen.getByText("Drop"));
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getByText("Tap to confirm")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Tap to confirm"));
    // Drop is now delayed by the undo window — no API call yet.
    expect(spy).not.toHaveBeenCalled();
    vi.advanceTimersByTime(DROP_UNDO_MS);
    await vi.runAllTimersAsync();
    expect(spy).toHaveBeenCalledOnce();
  });

  it("delays the Drop API until the undo window elapses, then dismisses", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ActionBar actions={[drop]} handle="web/x" onResult={onResult} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByText("Drop"));
    fireEvent.click(screen.getByText("Tap to confirm"));
    // After confirm, the API is not called yet; an undo toast is surfaced.
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

  it("Undo cancels the pending Drop without calling the API", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ActionBar actions={[drop]} handle="web/x" onResult={onResult} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByText("Drop"));
    fireEvent.click(screen.getByText("Tap to confirm"));
    const options = onResult.mock.calls[0][3] as { onUndo: () => void };
    options.onUndo();
    vi.advanceTimersByTime(DROP_UNDO_MS);
    await vi.runAllTimersAsync();
    expect(spy).not.toHaveBeenCalled();
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("expires the confirmation after the timeout", async () => {
    const spy = vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    render(<ActionBar actions={[drop]} handle="web/x" />);
    fireEvent.click(screen.getByText("Drop"));
    expect(screen.getByText("Tap to confirm")).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(8000);
    });
    expect(screen.queryByText("Tap to confirm")).toBeNull();
    expect(spy).not.toHaveBeenCalled();
  });

  it("routes to dismiss instead of refresh after a successful drop", async () => {
    vi.spyOn(api, "postOperation").mockResolvedValue({ ok: true, response: {} });
    const onMutated = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ActionBar actions={[drop]} handle="web/x" onMutated={onMutated} onDismiss={onDismiss} />,
    );
    fireEvent.click(screen.getByText("Drop"));
    fireEvent.click(screen.getByText("Tap to confirm"));
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
});
