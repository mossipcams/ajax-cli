import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { createRefitScheduler, RESIZE_DEBOUNCE_MS, type RefitScheduler } from "./terminalRefit";

const FRAME_MS = 16;

describe("RESIZE_DEBOUNCE_MS", () => {
  it("is 100ms", () => {
    expect(RESIZE_DEBOUNCE_MS).toBe(100);
  });
});

describe("createRefitScheduler", () => {
  let fit: Mock<() => void>;
  let sendResize: Mock<() => void>;
  let scheduler: RefitScheduler;

  beforeEach(() => {
    vi.useFakeTimers();
    fit = vi.fn<() => void>();
    sendResize = vi.fn<() => void>();
    scheduler = createRefitScheduler({ fit, sendResize });
  });

  afterEach(() => {
    scheduler.dispose();
    vi.useRealTimers();
  });

  it("immediate: fits and resizes together on the next frame", () => {
    scheduler.scheduleImmediate();
    expect(fit).not.toHaveBeenCalled();

    vi.advanceTimersByTime(FRAME_MS);

    expect(fit).toHaveBeenCalledTimes(1);
    expect(sendResize).toHaveBeenCalledTimes(1);
  });

  it("coalesces same-frame immediate requests into one fit", () => {
    scheduler.scheduleImmediate();
    scheduler.scheduleImmediate();

    vi.advanceTimersByTime(FRAME_MS);

    expect(fit).toHaveBeenCalledTimes(1);
    expect(sendResize).toHaveBeenCalledTimes(1);
  });

  it("debounced: fits per frame but sends one resize after the quiet window", () => {
    scheduler.scheduleDebounced();
    vi.advanceTimersByTime(FRAME_MS);
    scheduler.scheduleDebounced();
    vi.advanceTimersByTime(FRAME_MS);

    expect(fit).toHaveBeenCalledTimes(2);
    expect(sendResize).not.toHaveBeenCalled();

    vi.advanceTimersByTime(RESIZE_DEBOUNCE_MS);

    expect(sendResize).toHaveBeenCalledTimes(1);
  });

  it("restarts the resize debounce on every debounced request", () => {
    scheduler.scheduleDebounced();
    vi.advanceTimersByTime(RESIZE_DEBOUNCE_MS - 50);
    scheduler.scheduleDebounced();
    vi.advanceTimersByTime(RESIZE_DEBOUNCE_MS - 50);

    expect(sendResize).not.toHaveBeenCalled();

    vi.advanceTimersByTime(50);

    expect(sendResize).toHaveBeenCalledTimes(1);
  });

  it("font-size: fits on two consecutive frames for settling metrics", () => {
    scheduler.scheduleFontSize();

    vi.advanceTimersByTime(FRAME_MS);
    expect(fit).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(FRAME_MS);
    expect(fit).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(RESIZE_DEBOUNCE_MS);
    expect(sendResize).toHaveBeenCalledTimes(1);
  });

  it("post-layout: fits and resizes on two consecutive frames", () => {
    scheduler.schedulePostLayout();

    vi.advanceTimersByTime(FRAME_MS);
    expect(fit).toHaveBeenCalledTimes(1);
    expect(sendResize).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(FRAME_MS);
    expect(fit).toHaveBeenCalledTimes(2);
    expect(sendResize).toHaveBeenCalledTimes(2);
  });

  it("an immediate fit supersedes a pending debounced fit; the debounce still delivers its resize", () => {
    scheduler.scheduleDebounced();
    scheduler.scheduleImmediate();

    vi.advanceTimersByTime(FRAME_MS);
    expect(fit).toHaveBeenCalledTimes(1);
    expect(sendResize).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(RESIZE_DEBOUNCE_MS);
    expect(sendResize).toHaveBeenCalledTimes(2);
  });

  it("dispose cancels pending fits, resizes, and follow-up frames", () => {
    scheduler.scheduleFontSize();
    scheduler.dispose();

    vi.advanceTimersByTime(FRAME_MS * 4 + RESIZE_DEBOUNCE_MS);

    expect(fit).not.toHaveBeenCalled();
    expect(sendResize).not.toHaveBeenCalled();
  });

  it("ignores every schedule call after dispose", () => {
    scheduler.dispose();
    scheduler.scheduleImmediate();
    scheduler.scheduleDebounced();
    scheduler.scheduleFontSize();
    scheduler.schedulePostLayout();

    vi.advanceTimersByTime(FRAME_MS * 4 + RESIZE_DEBOUNCE_MS);

    expect(fit).not.toHaveBeenCalled();
    expect(sendResize).not.toHaveBeenCalled();
  });
});
