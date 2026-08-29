import { describe, it, expect } from "vitest";
import {
  navigateSwipeStart,
  navigateSwipeMove,
  navigateSwipeEnd,
  navigateSwipeTranslateX,
  navigateSwipeCommitOffset,
  crossSlideRemainingPx,
  crossSlideEnteringOffset,
  NAVIGATE_SWIPE_TRIGGER,
  isDiffPanGestureTarget,
} from "./navigateSwipe";

const PAGE_WIDTH = 390;

describe("navigate swipe", () => {
  it("does not engage below the horizontal dead-zone", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), 40, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    expect(navigateSwipeEnd(state)).toBe("none");
    expect(navigateSwipeTranslateX(state)).toBe(0);
  });

  it("ignores predominantly vertical drags", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -50, -80, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    expect(navigateSwipeEnd(state)).toBe("none");
  });

  it("fires left past the trigger and exposes translate", () => {
    let state = navigateSwipeMove(
      navigateSwipeStart(),
      -(NAVIGATE_SWIPE_TRIGGER + 1),
      0,
      PAGE_WIDTH,
    );
    expect(state.engaged).toBe(true);
    expect(navigateSwipeEnd(state)).toBe("left");
    expect(navigateSwipeTranslateX(state)).toBe(0);
    state = navigateSwipeMove(state, -(NAVIGATE_SWIPE_TRIGGER + 10), 0, PAGE_WIDTH);
    expect(navigateSwipeTranslateX(state)).toBe(-9);
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

  it("starts translate at zero when engagement crosses the dead-zone", () => {
    const atThreshold = navigateSwipeMove(navigateSwipeStart(), 48, 0, PAGE_WIDTH);
    expect(atThreshold.engaged).toBe(true);
    expect(navigateSwipeTranslateX(atThreshold)).toBe(0);

    const pastThreshold = navigateSwipeMove(atThreshold, 49, 0, PAGE_WIDTH);
    expect(navigateSwipeTranslateX(pastThreshold)).toBe(1);
  });

  it("tracks finger 1:1 after engagement", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH);
    expect(navigateSwipeTranslateX(state)).toBe(0);
    state = navigateSwipeMove(state, -200, 0, PAGE_WIDTH);
    expect(navigateSwipeTranslateX(state)).toBe(-152);
  });

  it("exposes cross-slide offsets from the current drag position", () => {
    expect(crossSlideEnteringOffset("left", -100, PAGE_WIDTH)).toBe(290);
    expect(crossSlideRemainingPx("left", -100, PAGE_WIDTH)).toBe(290);
    expect(crossSlideEnteringOffset("right", 100, PAGE_WIDTH)).toBe(-290);
    expect(crossSlideRemainingPx("right", 100, PAGE_WIDTH)).toBe(290);
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
