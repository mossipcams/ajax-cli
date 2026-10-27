import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WebAction } from "@/shared/lib/types";
import { DROP_UNDO_MS } from "@/shared/lib/polling";
import { armDropUndo, type DropUndoHandles } from "./taskMutations";

const dropAction = { action: "drop" } as WebAction;

function freshHandles(): DropUndoHandles {
  return {
    dropTimerRef: { current: null },
    dropResolvedRef: { current: false },
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
});
