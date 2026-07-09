import { describe, it, expect, vi } from "vitest";
import {
  scrollbackGrowthCompensation,
  outputFollowEffects,
  validTerminalSize,
  createResizeDedupe,
  createTerminalWriteBatcher,
  TERMINAL_WRITE_FLUSH_MS,
  TERMINAL_WRITE_MAX_CHARS,
} from "./terminalOutputPolicy";

describe("terminalOutputPolicy", () => {
  it("compensates positive scrollback growth while preserving reader position", () => {
    expect(scrollbackGrowthCompensation(40, 42)).toBe(-2);
    expect(scrollbackGrowthCompensation(40, 40)).toBe(0);
    expect(scrollbackGrowthCompensation(42, 40)).toBe(0);
    expect(scrollbackGrowthCompensation(NaN, 42)).toBe(0);
    expect(scrollbackGrowthCompensation(40, NaN)).toBe(0);
    expect(scrollbackGrowthCompensation(Infinity, 42)).toBe(0);
    expect(scrollbackGrowthCompensation(40, Infinity)).toBe(0);
  });

  it("maps pinned state to output follow effects", () => {
    expect(outputFollowEffects(true)).toEqual({
      snapToBottom: true,
      markUnseenOutput: false,
    });
    expect(outputFollowEffects(false)).toEqual({
      snapToBottom: false,
      markUnseenOutput: true,
    });
  });

  it("accepts only finite positive integer resize sizes", () => {
    expect(validTerminalSize(80, 24)).toEqual({ cols: 80, rows: 24 });
    expect(validTerminalSize(NaN, 24)).toBeUndefined();
    expect(validTerminalSize(80, NaN)).toBeUndefined();
    expect(validTerminalSize(Infinity, 24)).toBeUndefined();
    expect(validTerminalSize(80, Infinity)).toBeUndefined();
    expect(validTerminalSize(0, 24)).toBeUndefined();
    expect(validTerminalSize(80, 0)).toBeUndefined();
    expect(validTerminalSize(-1, 24)).toBeUndefined();
    expect(validTerminalSize(80, -1)).toBeUndefined();
    expect(validTerminalSize(80.5, 24)).toBeUndefined();
    expect(validTerminalSize(80, 24.5)).toBeUndefined();
  });

  it("createTerminalWriteBatcher coalesces pushes until flush timer fires", () => {
    const onFlush = vi.fn();
    let scheduled: (() => void) | undefined;
    const batcher = createTerminalWriteBatcher({
      onFlush,
      schedule: (fn) => {
        scheduled = fn;
        return 1 as ReturnType<typeof setTimeout>;
      },
      clearSchedule: () => {
        scheduled = undefined;
      },
    });

    batcher.push("a");
    batcher.push("b");
    expect(onFlush).not.toHaveBeenCalled();
    expect(batcher.pendingChars()).toBe(2);

    scheduled?.();
    expect(onFlush).toHaveBeenCalledTimes(1);
    expect(onFlush).toHaveBeenCalledWith("ab");
  });

  it("createTerminalWriteBatcher flushes immediately when max chars is reached", () => {
    const onFlush = vi.fn();
    const batcher = createTerminalWriteBatcher({
      onFlush,
      maxChars: 5,
      schedule: () => 1 as ReturnType<typeof setTimeout>,
      clearSchedule: () => {},
    });

    batcher.push("123");
    expect(onFlush).not.toHaveBeenCalled();
    batcher.push("45");
    expect(onFlush).toHaveBeenCalledTimes(1);
    expect(onFlush).toHaveBeenCalledWith("12345");
    expect(batcher.pendingChars()).toBe(0);
  });

  it("createTerminalWriteBatcher flush delivers one combined string and clears the queue", () => {
    const onFlush = vi.fn();
    let scheduled: (() => void) | undefined;
    const batcher = createTerminalWriteBatcher({
      onFlush,
      schedule: (fn) => {
        scheduled = fn;
        return 1 as ReturnType<typeof setTimeout>;
      },
      clearSchedule: () => {
        scheduled = undefined;
      },
    });

    batcher.push("hello");
    batcher.push("world");
    scheduled?.();
    expect(onFlush).toHaveBeenCalledTimes(1);
    expect(onFlush).toHaveBeenCalledWith("helloworld");
    expect(batcher.pendingChars()).toBe(0);

    onFlush.mockClear();
    batcher.flush();
    expect(onFlush).not.toHaveBeenCalled();
  });

  it("createTerminalWriteBatcher dispose cancels a pending timer without flushing", () => {
    const onFlush = vi.fn();
    let scheduled: (() => void) | undefined;
    let cleared = false;
    const batcher = createTerminalWriteBatcher({
      onFlush,
      schedule: (fn) => {
        scheduled = fn;
        return 1 as ReturnType<typeof setTimeout>;
      },
      clearSchedule: () => {
        cleared = true;
        scheduled = undefined;
      },
    });

    batcher.push("x");
    expect(batcher.pendingChars()).toBe(1);
    batcher.dispose();
    expect(cleared).toBe(true);
    expect(onFlush).not.toHaveBeenCalled();
    expect(batcher.pendingChars()).toBe(0);
    scheduled?.();
    expect(onFlush).not.toHaveBeenCalled();
  });

  it("TERMINAL_WRITE_FLUSH_MS is 16 and TERMINAL_WRITE_MAX_CHARS is 32000", () => {
    expect(TERMINAL_WRITE_FLUSH_MS).toBe(16);
    expect(TERMINAL_WRITE_MAX_CHARS).toBe(32_000);
  });

  it("createResizeDedupe skips send when cols and rows unchanged", () => {
    const send = vi.fn();
    const dedupe = createResizeDedupe(send);

    dedupe.sendIfChanged(80, 24);
    dedupe.sendIfChanged(80, 24);

    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith(80, 24);
  });

  it("createResizeDedupe sends when cols or rows change", () => {
    const send = vi.fn();
    const dedupe = createResizeDedupe(send);

    dedupe.sendIfChanged(80, 24);
    dedupe.sendIfChanged(81, 24);
    dedupe.sendIfChanged(81, 25);

    expect(send).toHaveBeenCalledTimes(3);
    expect(send).toHaveBeenNthCalledWith(1, 80, 24);
    expect(send).toHaveBeenNthCalledWith(2, 81, 24);
    expect(send).toHaveBeenNthCalledWith(3, 81, 25);
  });

  it("createResizeDedupe reset clears last-sent so same size can send again", () => {
    const send = vi.fn();
    const dedupe = createResizeDedupe(send);

    dedupe.sendIfChanged(80, 24);
    dedupe.sendIfChanged(80, 24);
    dedupe.reset();
    dedupe.sendIfChanged(80, 24);

    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenNthCalledWith(1, 80, 24);
    expect(send).toHaveBeenNthCalledWith(2, 80, 24);
  });
});
