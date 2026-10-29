import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WebAction } from "@/shared/lib/types";
import { DROP_UNDO_MS } from "@/shared/lib/polling";
import { armDropUndo, commitConfirmedAction, type DropUndoHandles } from "./taskMutations";

const dropAction = { action: "drop" } as WebAction;

function freshHandles(): DropUndoHandles {
  return {
    dropTimerRef: { current: null },
  };
}

describe("armDropUndo", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("posts both drops when a second drop is armed inside the first undo window", async () => {
    const executeOperation = vi.fn().mockResolvedValue({
      ok: true,
      response: {},
    });
    const callbacks = { onResult: vi.fn(), onDismiss: vi.fn() };
    const handles = freshHandles();

    armDropUndo(dropAction, "web/first", null, callbacks, handles, executeOperation);
    await vi.advanceTimersByTimeAsync(1000);
    armDropUndo(dropAction, "web/second", null, callbacks, handles, executeOperation);

    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);

    expect(executeOperation).toHaveBeenCalledTimes(2);
    expect(executeOperation).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ task_handle: "web/first", action: "drop", confirmed: true }),
    );
    expect(executeOperation).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ task_handle: "web/second", action: "drop", confirmed: true }),
    );
  });

  it("keeps the second drop's undo window independent of the first", async () => {
    const executeOperation = vi.fn().mockResolvedValue({
      ok: true,
      response: {},
    });
    const callbacks = { onResult: vi.fn(), onDismiss: vi.fn() };
    const handles = freshHandles();

    armDropUndo(dropAction, "web/first", null, callbacks, handles, executeOperation);
    await vi.advanceTimersByTimeAsync(1000);
    armDropUndo(dropAction, "web/second", null, callbacks, handles, executeOperation);

    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS - 1000);
    expect(executeOperation).toHaveBeenCalledTimes(1);
    expect(executeOperation).toHaveBeenLastCalledWith(
      expect.objectContaining({ task_handle: "web/first" }),
    );

    await vi.advanceTimersByTimeAsync(1000);
    expect(executeOperation).toHaveBeenCalledTimes(2);
    expect(executeOperation).toHaveBeenLastCalledWith(
      expect.objectContaining({ task_handle: "web/second" }),
    );
  });

  it("undoing the second drop does not cancel the first", async () => {
    const executeOperation = vi.fn().mockResolvedValue({
      ok: true,
      response: {},
    });
    const results: Array<{ onUndo?: () => void }> = [];
    const callbacks = {
      onResult: vi.fn((_message, _output, _isError, options) => {
        results.push(options ?? {});
      }),
      onDismiss: vi.fn(),
    };
    const handles = freshHandles();

    armDropUndo(dropAction, "web/first", null, callbacks, handles, executeOperation);
    await vi.advanceTimersByTimeAsync(1000);
    armDropUndo(dropAction, "web/second", null, callbacks, handles, executeOperation);
    results[1]?.onUndo?.();

    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);

    expect(executeOperation).toHaveBeenCalledTimes(1);
    expect(executeOperation).toHaveBeenCalledWith(
      expect.objectContaining({ task_handle: "web/first" }),
    );
  });

  it("commits immediately when the page becomes hidden during the undo window", async () => {
    const executeOperation = vi.fn().mockResolvedValue({ ok: true, response: {} });
    const callbacks = { onResult: vi.fn(), onDismiss: vi.fn() };
    const handles = freshHandles();
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });

    armDropUndo(dropAction, "web/x", null, callbacks, handles, executeOperation);
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(0);

    expect(executeOperation).toHaveBeenCalledOnce();
    expect(executeOperation).toHaveBeenCalledWith(
      expect.objectContaining({ task_handle: "web/x", action: "drop", confirmed: true }),
    );

    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);
    expect(executeOperation).toHaveBeenCalledOnce();
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  });

  it("commits immediately on pagehide during the undo window", async () => {
    const executeOperation = vi.fn().mockResolvedValue({ ok: true, response: {} });
    const callbacks = { onResult: vi.fn(), onDismiss: vi.fn() };
    const handles = freshHandles();

    armDropUndo(dropAction, "web/x", null, callbacks, handles, executeOperation);
    window.dispatchEvent(new Event("pagehide"));
    await vi.advanceTimersByTimeAsync(0);

    expect(executeOperation).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);
    expect(executeOperation).toHaveBeenCalledOnce();
  });

  it("does not commit a second time when the timer fires after a hide commit", async () => {
    const executeOperation = vi.fn().mockResolvedValue({ ok: true, response: {} });
    const callbacks = { onResult: vi.fn(), onDismiss: vi.fn() };
    const handles = freshHandles();
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });

    armDropUndo(dropAction, "web/x", null, callbacks, handles, executeOperation);
    document.dispatchEvent(new Event("visibilitychange"));
    window.dispatchEvent(new Event("pagehide"));
    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS * 2);

    expect(executeOperation).toHaveBeenCalledOnce();
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
  });

  it("ignores hide events after Undo settles the drop", async () => {
    const executeOperation = vi.fn().mockResolvedValue({ ok: true, response: {} });
    const results: Array<{ onUndo?: () => void }> = [];
    const callbacks = {
      onResult: vi.fn((_message, _output, _isError, options) => {
        results.push(options ?? {});
      }),
      onDismiss: vi.fn(),
    };
    const handles = freshHandles();

    armDropUndo(dropAction, "web/x", null, callbacks, handles, executeOperation);
    results[0]?.onUndo?.();
    window.dispatchEvent(new Event("pagehide"));
    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);

    expect(executeOperation).not.toHaveBeenCalled();
  });
});

describe("commitConfirmedAction", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("defers merge-pr through the undo window before posting", async () => {
    const executeOperation = vi.fn().mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const callbacks = { onResult, onMutated: vi.fn() };
    const mergeAction = { action: "merge-pr", label: "Merge PR" } as WebAction;

    commitConfirmedAction(mergeAction, "web/x", "tap-1", callbacks, freshHandles(), executeOperation);

    expect(executeOperation).not.toHaveBeenCalled();
    expect(onResult).toHaveBeenCalledWith(
      "Merge PR for web/x…",
      null,
      false,
      expect.objectContaining({ onUndo: expect.any(Function) }),
    );

    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS - 1);
    expect(executeOperation).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(executeOperation).toHaveBeenCalledOnce();
    expect(executeOperation).toHaveBeenCalledWith(
      expect.objectContaining({ task_handle: "web/x", action: "merge-pr", confirmed: true }),
    );
  });

  it("cancels the merge-pr post when Undo is pressed", async () => {
    const executeOperation = vi.fn().mockResolvedValue({ ok: true, response: {} });
    const results: Array<{ onUndo?: () => void }> = [];
    const callbacks = {
      onResult: vi.fn((_message, _output, _isError, options) => {
        results.push(options ?? {});
      }),
      onMutated: vi.fn(),
    };
    const mergeAction = { action: "merge-pr", label: "Merge PR" } as WebAction;

    commitConfirmedAction(mergeAction, "web/x", "tap-1", callbacks, freshHandles(), executeOperation);
    results[0]?.onUndo?.();
    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);

    expect(executeOperation).not.toHaveBeenCalled();
  });

  it("defers create-pr through the undo window before posting", async () => {
    const executeOperation = vi.fn().mockResolvedValue({ ok: true, response: {} });
    const onResult = vi.fn();
    const callbacks = { onResult, onMutated: vi.fn() };
    const createAction = { action: "create-pr", label: "Create PR" } as WebAction;

    commitConfirmedAction(createAction, "web/x", "tap-1", callbacks, freshHandles(), executeOperation);

    expect(executeOperation).not.toHaveBeenCalled();
    expect(onResult).toHaveBeenCalledWith(
      "Create PR for web/x…",
      null,
      false,
      expect.objectContaining({ onUndo: expect.any(Function) }),
    );

    await vi.advanceTimersByTimeAsync(DROP_UNDO_MS);
    expect(executeOperation).toHaveBeenCalledOnce();
    expect(executeOperation).toHaveBeenCalledWith(
      expect.objectContaining({ task_handle: "web/x", action: "create-pr", confirmed: true }),
    );
  });
});
