import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  KEY_REPEAT_INITIAL_DELAY_MS,
  KEY_REPEAT_INITIAL_INTERVAL_MS,
  KEY_REPEAT_MIN_INTERVAL_MS,
  KEY_REPEAT_STAGE_INTERVALS_MS,
  nextRepeatInterval,
  createHeldKeyRepeater,
} from "./keyRepeat";

describe("nextRepeatInterval", () => {
  it("stays within bounds and decreases across stages to the minimum", () => {
    const seen = new Set<number>();
    for (let stage = 0; stage < 20; stage += 1) {
      const interval = nextRepeatInterval(stage);
      expect(interval).toBeGreaterThanOrEqual(KEY_REPEAT_MIN_INTERVAL_MS);
      expect(interval).toBeLessThanOrEqual(KEY_REPEAT_INITIAL_INTERVAL_MS);
      seen.add(interval);
    }
    expect(seen.has(KEY_REPEAT_MIN_INTERVAL_MS)).toBe(true);
    expect(nextRepeatInterval(0)).toBe(KEY_REPEAT_STAGE_INTERVALS_MS[0]);
    expect(nextRepeatInterval(19)).toBe(KEY_REPEAT_MIN_INTERVAL_MS);
  });
});

describe("createHeldKeyRepeater", () => {
  let now: number;
  let timers: Array<{ id: number; at: number; fn: () => void }>;
  let nextId: number;

  beforeEach(() => {
    vi.useFakeTimers();
    now = 0;
    nextId = 1;
    timers = [];
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function fakeSetTimeout(fn: () => void, ms?: number) {
    const id = nextId++;
    timers.push({ id, at: now + (ms ?? 0), fn });
    return id as unknown as ReturnType<typeof setTimeout>;
  }

  function fakeClearTimeout(id: ReturnType<typeof setTimeout>) {
    const numeric = id as unknown as number;
    const index = timers.findIndex((timer) => timer.id === numeric);
    if (index >= 0) timers.splice(index, 1);
  }

  function advanceTo(target: number) {
    while (timers.length > 0) {
      timers.sort((a, b) => a.at - b.at);
      const next = timers[0];
      if (next.at > target) break;
      now = next.at;
      timers.shift();
      next.fn();
    }
    now = target;
  }

  it("emits once immediately on start", () => {
    const emit = vi.fn();
    const repeater = createHeldKeyRepeater({
      emit,
      setTimeout: fakeSetTimeout,
      clearTimeout: fakeClearTimeout,
    });
    repeater.start();
    expect(emit).toHaveBeenCalledTimes(1);
  });

  it("does not emit again before the initial delay", () => {
    const emit = vi.fn();
    const repeater = createHeldKeyRepeater({
      emit,
      setTimeout: fakeSetTimeout,
      clearTimeout: fakeClearTimeout,
    });
    repeater.start();
    advanceTo(KEY_REPEAT_INITIAL_DELAY_MS - 1);
    expect(emit).toHaveBeenCalledTimes(1);
  });

  it("emits at accelerating intervals after the initial delay", () => {
    const emit = vi.fn();
    const repeater = createHeldKeyRepeater({
      emit,
      setTimeout: fakeSetTimeout,
      clearTimeout: fakeClearTimeout,
    });
    repeater.start();
    advanceTo(KEY_REPEAT_INITIAL_DELAY_MS);
    expect(emit).toHaveBeenCalledTimes(2);
    advanceTo(KEY_REPEAT_INITIAL_DELAY_MS + nextRepeatInterval(0));
    expect(emit).toHaveBeenCalledTimes(3);
    advanceTo(
      KEY_REPEAT_INITIAL_DELAY_MS + nextRepeatInterval(0) + nextRepeatInterval(1),
    );
    expect(emit).toHaveBeenCalledTimes(4);
  });

  it("stop prevents further emits", () => {
    const emit = vi.fn();
    const repeater = createHeldKeyRepeater({
      emit,
      setTimeout: fakeSetTimeout,
      clearTimeout: fakeClearTimeout,
    });
    repeater.start();
    repeater.stop();
    advanceTo(KEY_REPEAT_INITIAL_DELAY_MS + 500);
    expect(emit).toHaveBeenCalledTimes(1);
  });

  it("stops when emit observes isActive() === false", () => {
    let active = true;
    const emit = vi.fn(() => {
      active = false;
    });
    const repeater = createHeldKeyRepeater({
      emit,
      isActive: () => active,
      setTimeout: fakeSetTimeout,
      clearTimeout: fakeClearTimeout,
    });
    repeater.start();
    advanceTo(KEY_REPEAT_INITIAL_DELAY_MS + 500);
    expect(emit).toHaveBeenCalledTimes(1);
  });
});
