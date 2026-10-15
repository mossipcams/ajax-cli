import { describe, it, expect } from "vitest";
import {
  anchorIsStale,
  autoLoadDecision,
  restoreScrollAfterTopGrowth,
  scrollHeightDelta,
  AUTO_LOAD_COOLDOWN_MS,
} from "./historyScroll";

function thread(scrollTop: number, scrollHeight: number, clientHeight: number): HTMLDivElement {
  const node = document.createElement("div");
  Object.defineProperty(node, "scrollHeight", { configurable: true, value: scrollHeight });
  Object.defineProperty(node, "clientHeight", { configurable: true, value: clientHeight });
  node.scrollTop = scrollTop;
  return node;
}

describe("historyScroll", () => {
  it("computes scrollHeight delta", () => {
    expect(scrollHeightDelta(1200, 1300)).toBe(100);
    expect(scrollHeightDelta(1200, 1200)).toBe(0);
  });

  it("restores read position after prepend-style top growth (reversed coords)", () => {
    // Reversed: live edge is 0, reading 300px away from it.
    const el = thread(-300, 1300, 400);
    restoreScrollAfterTopGrowth(el, -300, 1200);
    expect(el.scrollTop).toBe(-400);
  });

  it("moves pinned non-overflowing windows negative after prepend growth", () => {
    // Pinned at the live edge (scrollTop = 0) in a window that fit; the reveal
    // grows scrollHeight past clientHeight, so distance from the live edge must
    // become -delta.
    const el = thread(0, 500, 400);
    restoreScrollAfterTopGrowth(el, 0, 300);
    expect(el.scrollTop).toBe(-200);
  });

  it("ignores shrink or zero delta", () => {
    const el = thread(-120, 1100, 400);
    restoreScrollAfterTopGrowth(el, -120, 1200);
    expect(el.scrollTop).toBe(-120);
  });

  it("marks empty reveals as stale anchors", () => {
    expect(anchorIsStale(1200, 1300, 0)).toBe(true);
    expect(anchorIsStale(1200, 1200, 5)).toBe(true);
    expect(anchorIsStale(1200, 1300, 3)).toBe(false);
  });

  it("does not auto-load without overflow", () => {
    const el = thread(0, 300, 400);
    const result = autoLoadDecision(el, { armed: true, lastLoadAt: 0 }, true, 1000);
    expect(result.shouldLoad).toBe(false);
  });

  it("does not auto-load at the live edge until armed", () => {
    // Reversed coords: scrollTop = 0 is the live edge; it also re-arms.
    const el = thread(0, 1200, 400);
    const result = autoLoadDecision(el, { armed: false, lastLoadAt: 0 }, true, 1000);
    expect(result.shouldLoad).toBe(false);
    expect(result.nextState.armed).toBe(true);
  });

  it("auto-loads near the history edge when armed and cooldown elapsed", () => {
    // Reversed coords (SH=1200, CH=400): live edge = 0, history edge = -800,
    // preload band is [-800, -600].
    const el = thread(0, 1200, 400);
    const armed = autoLoadDecision(el, { armed: false, lastLoadAt: 0 }, true, 1000);
    expect(armed.shouldLoad).toBe(false);
    expect(armed.nextState.armed).toBe(true);

    // Mid-history is away from the history edge: still armed, no load.
    const scrolled = thread(-400, 1200, 400);
    const afterScroll = autoLoadDecision(scrolled, armed.nextState, true, 1000);
    expect(afterScroll.shouldLoad).toBe(false);
    expect(afterScroll.nextState.armed).toBe(true);

    const nearEdge = thread(-750, 1200, 400);
    const load = autoLoadDecision(nearEdge, afterScroll.nextState, true, AUTO_LOAD_COOLDOWN_MS + 10);
    expect(load.shouldLoad).toBe(true);
  });

  it("respects auto-load cooldown", () => {
    const el = thread(-750, 1200, 400);
    const state = { armed: true, lastLoadAt: 1000 };
    const blocked = autoLoadDecision(el, state, true, 1200);
    expect(blocked.shouldLoad).toBe(false);
  });

  it("disarms after fire and does not reload near the edge until re-armed", () => {
    const armed = { armed: true, lastLoadAt: 0 };
    const nearEdge = thread(-750, 1200, 400);
    const fired = autoLoadDecision(nearEdge, armed, true, AUTO_LOAD_COOLDOWN_MS + 10);
    expect(fired.shouldLoad).toBe(true);
    expect(fired.nextState.armed).toBe(false);

    const immediate = autoLoadDecision(nearEdge, fired.nextState, true, AUTO_LOAD_COOLDOWN_MS + 20);
    expect(immediate.shouldLoad).toBe(false);
    expect(immediate.nextState.armed).toBe(false);

    // Moving back toward the live edge re-arms for the next pass.
    const scrolled = thread(0, 1200, 400);
    const rearmed = autoLoadDecision(scrolled, immediate.nextState, true, AUTO_LOAD_COOLDOWN_MS + 30);
    expect(rearmed.shouldLoad).toBe(false);
    expect(rearmed.nextState.armed).toBe(true);

    const reload = autoLoadDecision(nearEdge, rearmed.nextState, true, AUTO_LOAD_COOLDOWN_MS * 2 + 30);
    expect(reload.shouldLoad).toBe(true);
  });
});
