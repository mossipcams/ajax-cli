import { describe, it, expect } from "vitest";
import {
  anchorIsStale,
  autoLoadDecision,
  restoreScrollAfterTopGrowth,
  scrollHeightDelta,
  HISTORY_PRELOAD_PX,
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

  it("restores read position after prepend-style top growth", () => {
    const el = thread(120, 1300, 400);
    restoreScrollAfterTopGrowth(el, 120, 1200);
    expect(el.scrollTop).toBe(220);
  });

  it("ignores shrink or zero delta", () => {
    const el = thread(120, 1100, 400);
    restoreScrollAfterTopGrowth(el, 120, 1200);
    expect(el.scrollTop).toBe(120);
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

  it("does not auto-load at scrollTop 0 until armed", () => {
    const el = thread(0, 1200, 400);
    const result = autoLoadDecision(el, { armed: false, lastLoadAt: 0 }, true, 1000);
    expect(result.shouldLoad).toBe(false);
  });

  it("auto-loads near the top when armed and cooldown elapsed", () => {
    const el = thread(HISTORY_PRELOAD_PX, 1200, 400);
    const armed = autoLoadDecision(el, { armed: false, lastLoadAt: 0 }, true, 1000);
    expect(armed.nextState.armed).toBe(false);

    const scrolled = thread(500, 1200, 400);
    const afterScroll = autoLoadDecision(scrolled, armed.nextState, true, 1000);
    expect(afterScroll.nextState.armed).toBe(true);

    const nearTop = thread(HISTORY_PRELOAD_PX, 1200, 400);
    const load = autoLoadDecision(nearTop, afterScroll.nextState, true, AUTO_LOAD_COOLDOWN_MS + 10);
    expect(load.shouldLoad).toBe(true);
  });

  it("respects auto-load cooldown", () => {
    const el = thread(HISTORY_PRELOAD_PX, 1200, 400);
    const state = { armed: true, lastLoadAt: 1000 };
    const blocked = autoLoadDecision(el, state, true, 1200);
    expect(blocked.shouldLoad).toBe(false);
  });

  it("disarms after fire and does not reload near top until re-armed", () => {
    const armed = { armed: true, lastLoadAt: 0 };
    const nearTop = thread(HISTORY_PRELOAD_PX, 1200, 400);
    const fired = autoLoadDecision(nearTop, armed, true, AUTO_LOAD_COOLDOWN_MS + 10);
    expect(fired.shouldLoad).toBe(true);
    expect(fired.nextState.armed).toBe(false);

    const immediate = autoLoadDecision(nearTop, fired.nextState, true, AUTO_LOAD_COOLDOWN_MS + 20);
    expect(immediate.shouldLoad).toBe(false);
    expect(immediate.nextState.armed).toBe(false);

    const scrolled = thread(500, 1200, 400);
    const rearmed = autoLoadDecision(scrolled, immediate.nextState, true, AUTO_LOAD_COOLDOWN_MS + 30);
    expect(rearmed.nextState.armed).toBe(true);

    const reload = autoLoadDecision(nearTop, rearmed.nextState, true, AUTO_LOAD_COOLDOWN_MS * 2 + 30);
    expect(reload.shouldLoad).toBe(true);
  });
});
