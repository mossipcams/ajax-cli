import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  crossSlideRemainingPx,
  NAVIGATE_SWIPE_TRIGGER,
} from "@/shared/gestures/navigateSwipe";
import {
  computeSwipeCommitDurationMs,
  scheduleCrossSlideAnimatingFlip,
  SERIAL_SWIPE_COMMIT_BUDGET_MS,
  SWIPE_PAGE_COMMIT_MS,
} from "./useSwipePageTransition";

const PAGE_WIDTH = 390;
const COMMIT_DRAG_X = -(NAVIGATE_SWIPE_TRIGGER + 8);

describe("swipe commit timing", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("documents the serial exit+enter budget cross-slide replaces", () => {
    expect(SERIAL_SWIPE_COMMIT_BUDGET_MS).toBe(440);
  });

  it("keeps the cross-slide commit inside one motion budget", () => {
    const remaining = crossSlideRemainingPx("left", COMMIT_DRAG_X, PAGE_WIDTH);
    const crossSlideMs = computeSwipeCommitDurationMs(remaining, 0);
    expect(crossSlideMs).toBeLessThanOrEqual(SWIPE_PAGE_COMMIT_MS);
    expect(crossSlideMs).toBeLessThan(SERIAL_SWIPE_COMMIT_BUDGET_MS);
  });

  it("shortens commit duration when average gesture velocity is high", () => {
    const remaining = crossSlideRemainingPx("left", COMMIT_DRAG_X, PAGE_WIDTH);
    const baselineMs = computeSwipeCommitDurationMs(remaining, 0);
    const flickMs = computeSwipeCommitDurationMs(remaining, 2.5);
    expect(flickMs).toBeLessThan(baselineMs);
    expect(flickMs).toBeGreaterThanOrEqual(80);
  });

  it("schedules armed-to-animating flip on the next task in jsdom", () => {
    const callback = vi.fn();
    scheduleCrossSlideAnimatingFlip(callback);
    expect(callback).not.toHaveBeenCalled();
    vi.runOnlyPendingTimers();
    expect(callback).toHaveBeenCalledOnce();
  });
});
