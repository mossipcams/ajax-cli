import { describe, it, expect } from "vitest";
import {
  swipeStart,
  swipeMove,
  swipeEnd,
  SWIPE_REVEAL_WIDTH,
  SWIPE_REVEAL_WIDTH_VAR,
  SWIPE_TRIGGER,
  REVEAL_AUTO_HIDE_MS,
} from "./swipeReveal";

describe("swipe-to-reveal gesture", () => {
  it("ignores predominantly vertical drags so the list can still scroll", () => {
    const state = swipeMove(swipeStart(), -10, -40);
    expect(state.engaged).toBe(false);
    expect(state.offset).toBe(0);
  });

  it("engages on a clearly horizontal left swipe and reveals proportionally", () => {
    const state = swipeMove(swipeStart(), -40, -4);
    expect(state.engaged).toBe(true);
    expect(state.offset).toBeGreaterThan(0);
  });

  it("clamps the reveal offset to the action width", () => {
    expect(swipeMove(swipeStart(), -500, 0).offset).toBe(SWIPE_REVEAL_WIDTH);
  });

  it("does not reveal on a rightward swipe from closed", () => {
    expect(swipeMove(swipeStart(), 80, 0).offset).toBe(0);
  });

  it("closes from an open offset on a rightward swipe", () => {
    const mid = swipeMove(swipeStart(SWIPE_REVEAL_WIDTH), 80, 0);
    expect(mid.engaged).toBe(true);
    expect(mid.offset).toBe(SWIPE_REVEAL_WIDTH - 80);

    const closed = swipeEnd(swipeMove(swipeStart(SWIPE_REVEAL_WIDTH), SWIPE_REVEAL_WIDTH, 0));
    expect(closed).toEqual({ open: false, offset: 0 });
  });

  it("exports the auto-hide duration for settled reveals", () => {
    expect(REVEAL_AUTO_HIDE_MS).toBe(10_000);
  });

  it("snaps open past the trigger and closed before it on release", () => {
    const opened = swipeMove(swipeStart(), -SWIPE_REVEAL_WIDTH, 0);
    expect(swipeEnd(opened)).toEqual({ open: true, offset: SWIPE_REVEAL_WIDTH });

    const barely = swipeMove(swipeStart(), -(SWIPE_TRIGGER - 1), 0);
    expect(swipeEnd(barely)).toEqual({ open: false, offset: 0 });
  });

  it("reserves enough width for the confirming Drop label", () => {
    // "Tap to confirm" + button padding + reveal padding-right must fit without clip.
    expect(SWIPE_REVEAL_WIDTH).toBeGreaterThanOrEqual(148);
  });

  it("exports a CSS variable name synced to dashboard row reveal width", () => {
    expect(SWIPE_REVEAL_WIDTH_VAR).toBe("--task-row-reveal-width");
  });
});
