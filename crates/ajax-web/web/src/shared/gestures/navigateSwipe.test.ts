import { describe, it, expect } from "vitest";
import {
  navigateSwipeStart,
  navigateSwipeMove,
  navigateSwipeEnd,
  NAVIGATE_SWIPE_TRIGGER,
  isTerminalGestureTarget,
  isDiffPanGestureTarget,
} from "./navigateSwipe";

describe("navigate swipe", () => {
  it("ignores predominantly vertical drags", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -12, -40);
    expect(state.engaged).toBe(false);
    expect(navigateSwipeEnd(state)).toBe("none");
  });

  it("fires left past the trigger", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -(NAVIGATE_SWIPE_TRIGGER + 1), 0);
    expect(state.engaged).toBe(true);
    expect(navigateSwipeEnd(state)).toBe("left");
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

describe("isTerminalGestureTarget", () => {
  it("detects terminal surfaces", () => {
    const panel = document.createElement("div");
    panel.setAttribute("data-testid", "task-terminal-panel");
    const child = document.createElement("span");
    panel.appendChild(child);
    document.body.appendChild(panel);
    expect(isTerminalGestureTarget(child)).toBe(true);
    expect(isTerminalGestureTarget(document.body)).toBe(false);
    panel.remove();
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
