import { describe, it, expect } from "vitest";
import {
  navigateSwipeStart,
  navigateSwipeMove,
  navigateSwipeEnd,
  navigateSwipeTranslateX,
  navigateSwipeCommitOffset,
  NAVIGATE_SWIPE_TRIGGER,
  isDiffPanGestureTarget,
} from "./navigateSwipe";

const PAGE_WIDTH = 390;

describe("navigate swipe", () => {
  it("ignores predominantly vertical drags", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -12, -40, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    expect(navigateSwipeEnd(state)).toBe("none");
  });

  it("fires left past the trigger and exposes translate", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -(NAVIGATE_SWIPE_TRIGGER + 1), 0, PAGE_WIDTH);
    expect(state.engaged).toBe(true);
    expect(navigateSwipeEnd(state)).toBe("left");
    expect(navigateSwipeTranslateX(state)).toBeLessThan(0);
  });

  it("fires right past the trigger", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), NAVIGATE_SWIPE_TRIGGER + 1, 2, PAGE_WIDTH);
    expect(navigateSwipeEnd(state)).toBe("right");
  });

  it("stays none before the trigger even when engaged", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -(NAVIGATE_SWIPE_TRIGGER - 1), 0, PAGE_WIDTH);
    expect(state.engaged).toBe(true);
    expect(navigateSwipeEnd(state)).toBe("none");
  });

  it("follows finger up to page width", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -200, 0, PAGE_WIDTH);
    expect(navigateSwipeTranslateX(state)).toBe(-200);
  });

  it("exposes commit offsets at full width", () => {
    expect(navigateSwipeCommitOffset("left", PAGE_WIDTH)).toBe(-PAGE_WIDTH);
    expect(navigateSwipeCommitOffset("right", PAGE_WIDTH)).toBe(PAGE_WIDTH);
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
