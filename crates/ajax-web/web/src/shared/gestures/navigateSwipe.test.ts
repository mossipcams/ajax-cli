import { describe, it, expect } from "vitest";
import {
  navigateSwipeStart,
  navigateSwipeMove,
  navigateSwipeEnd,
  navigateSwipeTranslateX,
  NAVIGATE_SWIPE_TRIGGER,
  NAVIGATE_LONG_PRESS_MS,
  NAVIGATE_LONG_PRESS_MOVE_CANCEL_PX,
  longPressArmed,
  isDiffPanGestureTarget,
} from "./navigateSwipe";

describe("navigate swipe", () => {
  it("ignores predominantly vertical drags", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -12, -40);
    expect(state.engaged).toBe(false);
    expect(navigateSwipeEnd(state)).toBe("none");
  });

  it("fires left past the trigger and exposes translate", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -(NAVIGATE_SWIPE_TRIGGER + 1), 0);
    expect(state.engaged).toBe(true);
    expect(navigateSwipeEnd(state)).toBe("left");
    expect(navigateSwipeTranslateX(state)).toBeLessThan(0);
  });

  it("fires right past the trigger", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), NAVIGATE_SWIPE_TRIGGER + 1, 2);
    expect(navigateSwipeEnd(state)).toBe("right");
  });

  it("stays none before the trigger even when engaged", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -(NAVIGATE_SWIPE_TRIGGER - 1), 0);
    expect(state.engaged).toBe(true);
    expect(navigateSwipeEnd(state)).toBe("none");
  });
});

describe("longPressArmed", () => {
  it("requires hold duration without early movement", () => {
    const start = 1_000;
    expect(longPressArmed(start, start + NAVIGATE_LONG_PRESS_MS - 1, 0)).toBe(false);
    expect(longPressArmed(start, start + NAVIGATE_LONG_PRESS_MS, 0)).toBe(true);
    expect(
      longPressArmed(start, start + NAVIGATE_LONG_PRESS_MS, NAVIGATE_LONG_PRESS_MOVE_CANCEL_PX),
    ).toBe(true);
    expect(
      longPressArmed(start, start + NAVIGATE_LONG_PRESS_MS, NAVIGATE_LONG_PRESS_MOVE_CANCEL_PX + 1),
    ).toBe(false);
  });
});

describe("isDiffPanGestureTarget", () => {
  it("detects PR chip strip and hunk surfaces", () => {
    const strip = document.createElement("div");
    strip.setAttribute("data-testid", "diff-pr-strip");
    const chip = document.createElement("button");
    strip.appendChild(chip);
    document.body.appendChild(strip);
    expect(isDiffPanGestureTarget(chip)).toBe(true);
    expect(isDiffPanGestureTarget(document.body)).toBe(false);
    strip.remove();
  });
});
