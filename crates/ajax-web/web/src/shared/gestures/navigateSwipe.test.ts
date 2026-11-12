import { describe, it, expect } from "vitest";
import {
  navigateSwipeStart,
  navigateSwipeMove,
  navigateSwipeEnd,
  navigateSwipeTranslateX,
  navigateSwipeCommitOffset,
  crossSlideRemainingPx,
  crossSlideEnteringOffset,
  releaseVelocity,
  isDiffPanGestureTarget,
} from "./navigateSwipe";

const PAGE_WIDTH = 390;
const HALF_PAGE = PAGE_WIDTH * 0.5;

describe("navigate swipe", () => {
  it("does not engage below the horizontal dead-zone", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), 40, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: -1 })).toBe("none");
    expect(navigateSwipeTranslateX(state)).toBe(0);
  });

  it("ignores predominantly vertical drags", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -50, -80, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: -1 })).toBe("none");
  });

  it("engages mid-screen only past the 48px dead-zone", () => {
    expect(navigateSwipeMove(navigateSwipeStart(), 47, 0, PAGE_WIDTH).engaged).toBe(false);
    expect(navigateSwipeMove(navigateSwipeStart(), 48, 0, PAGE_WIDTH).engaged).toBe(true);
    expect(navigateSwipeMove(navigateSwipeStart(), -47, 0, PAGE_WIDTH).engaged).toBe(false);
    expect(navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH).engaged).toBe(true);
  });

  it("fires left on a fast flick and exposes translate", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(true);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 0 })).toBe("none");
    expect(navigateSwipeTranslateX(state)).toBe(0);
    state = navigateSwipeMove(state, -58, 0, PAGE_WIDTH);
    expect(navigateSwipeTranslateX(state)).toBe(-10);
    state = navigateSwipeMove(state, -(48 + 10 + 9), 0, PAGE_WIDTH);
    expect(navigateSwipeTranslateX(state)).toBe(-19);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: -0.5 })).toBe("left");
  });

  it("fires right on a fast flick", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), 48, 2, PAGE_WIDTH);
    state = navigateSwipeMove(state, 100, 2, PAGE_WIDTH);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 0.5 })).toBe("right");
  });

  it("caps right commit when the list route is already underneath (#1064)", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), 48, 2, PAGE_WIDTH, {
      capRightCommit: true,
    });
    expect(state.engaged).toBe(false);
    expect(
      navigateSwipeEnd(state, { capRightCommit: true, pageWidth: PAGE_WIDTH, releaseVelocity: 5 }),
    ).toBe("none");

    let engaged = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH, {
      capRightCommit: true,
    });
    engaged = navigateSwipeMove(engaged, -100, 0, PAGE_WIDTH, { capRightCommit: true });
    expect(
      navigateSwipeEnd(engaged, { capRightCommit: true, pageWidth: PAGE_WIDTH, releaseVelocity: -0.5 }),
    ).toBe("left");
    engaged = navigateSwipeMove(engaged, 76, 0, PAGE_WIDTH, {
      capRightCommit: true,
    });
    expect(engaged.direction).toBe("none");
    expect(
      navigateSwipeEnd(engaged, { capRightCommit: true, pageWidth: PAGE_WIDTH, releaseVelocity: 5 }),
    ).toBe("none");
  });

  it("stays none before the commit point even when engaged", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH);
    state = navigateSwipeMove(state, -90, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(true);
    expect(state.direction).toBe("left");
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 0 })).toBe("none");
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

  it("engages a left-edge rightward swipe after 10px and tracks 1:1", () => {
    let state = navigateSwipeMove(navigateSwipeStart(true), 9, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    state = navigateSwipeMove(state, 10, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(true);
    expect(state.edgeStart).toBe(true);
    expect(navigateSwipeTranslateX(state)).toBe(0);
    state = navigateSwipeMove(state, 40, 0, PAGE_WIDTH);
    expect(state.edgeStart).toBe(true);
    expect(state.direction).toBe("right");
    expect(navigateSwipeTranslateX(state)).toBe(30);
  });

  it("keeps the 48px engage minimum for a leftward swipe from the edge", () => {
    let state = navigateSwipeMove(navigateSwipeStart(true), -10, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    state = navigateSwipeMove(state, -47, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    state = navigateSwipeMove(state, -48, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(true);
  });

  it("applies the lock ratio to an edge-start drag", () => {
    const state = navigateSwipeMove(navigateSwipeStart(true), 12, 20, PAGE_WIDTH);
    expect(state.engaged).toBe(false);
    expect(state.edgeStart).toBe(true);
  });

  it("commits a slow drag past 50% of the page width", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH);
    state = navigateSwipeMove(state, -(48 + HALF_PAGE + 7), 0, PAGE_WIDTH);
    expect(navigateSwipeTranslateX(state)).toBeLessThan(-HALF_PAGE);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 0 })).toBe("left");
  });

  it("springs back a slow drag under 50% of the page width", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH);
    state = navigateSwipeMove(state, -150, 0, PAGE_WIDTH);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 0 })).toBe("none");
  });

  it("commits at exactly 50% of the page width only when past it", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), 48, 0, PAGE_WIDTH);
    state = navigateSwipeMove(state, 48 + HALF_PAGE, 0, PAGE_WIDTH);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 0 })).toBe("none");
    state = navigateSwipeMove(state, 48 + HALF_PAGE + 1, 0, PAGE_WIDTH);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 0 })).toBe("right");
  });

  it("commits a short fast flick above the 0.2 px/ms velocity", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH);
    state = navigateSwipeMove(state, -100, 0, PAGE_WIDTH);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: -0.2 })).toBe("none");
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: -0.21 })).toBe("left");
  });

  it("commits a swipe whose engaging move is also its last move as a flick", () => {
    const state = navigateSwipeMove(navigateSwipeStart(), -80, 0, PAGE_WIDTH);
    expect(state.engaged).toBe(true);
    expect(navigateSwipeTranslateX(state)).toBe(0);
    const velocity = releaseVelocity([{ t: 0, x: 200 }, { t: 0, x: 120 }], 0);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: velocity })).toBe("left");
  });

  it("springs back a release moving backward past 50% of the page width", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH);
    state = navigateSwipeMove(state, -260, 0, PAGE_WIDTH);
    expect(Math.abs(navigateSwipeTranslateX(state))).toBeGreaterThan(HALF_PAGE);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 1 })).toBe("none");
  });

  it("springs back a backward flick when travel is under 50%", () => {
    let state = navigateSwipeMove(navigateSwipeStart(), -48, 0, PAGE_WIDTH);
    state = navigateSwipeMove(state, -100, 0, PAGE_WIDTH);
    expect(navigateSwipeEnd(state, { pageWidth: PAGE_WIDTH, releaseVelocity: 0.5 })).toBe("none");
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

describe("releaseVelocity", () => {
  it("is zero with fewer than two samples in the window", () => {
    expect(releaseVelocity([], 100)).toBe(0);
    expect(releaseVelocity([{ t: 100, x: 40 }], 100)).toBe(0);
  });

  it("uses only samples from the last 100ms", () => {
    const samples = [
      { t: 0, x: 0 },
      { t: 50, x: 10 },
      { t: 120, x: 30 },
    ];
    expect(releaseVelocity(samples, 130)).toBeCloseTo(20 / 70);
  });

  it("is zero when the finger paused before lifting", () => {
    const samples = [
      { t: 0, x: 300 },
      { t: 10, x: 250 },
    ];
    expect(releaseVelocity(samples, 200)).toBe(0);
  });

  it("returns a signed velocity", () => {
    const samples = [
      { t: 0, x: 300 },
      { t: 20, x: 200 },
    ];
    expect(releaseVelocity(samples, 20)).toBe(-5);
  });

  it("floors the time delta at 1ms", () => {
    const samples = [
      { t: 100, x: 0 },
      { t: 100, x: 5 },
    ];
    expect(releaseVelocity(samples, 100)).toBe(5);
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
