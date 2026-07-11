import { describe, it, expect, vi } from "vitest";
import {
  createTerminalLayoutPolicy,
  EXPAND_REWRAP_MS,
  type LayoutDecision,
} from "./terminalLayoutPolicy";

const frozen: LayoutDecision = {
  allowLocalFit: false,
  allowPtyResize: false,
  cropToBottom: true,
  pinToBottomOnKeyboardOpen: false,
};

const allowed: LayoutDecision = {
  allowLocalFit: true,
  allowPtyResize: true,
  cropToBottom: false,
  pinToBottomOnKeyboardOpen: false,
};

type Scheduled = { id: number; fireAt: number; fn: () => void };

const createTestHarness = () => {
  let now = 0;
  let nextScheduleId = 1;
  let nextRafId = 1;
  const scheduled: Scheduled[] = [];
  const rafQueue: Array<{ id: number; fn: () => void }> = [];

  const policy = createTerminalLayoutPolicy({
    now: () => now,
    schedule: (fn, delayMs) => {
      const id = nextScheduleId++;
      scheduled.push({ id, fireAt: now + delayMs, fn });
      return id as ReturnType<typeof setTimeout>;
    },
    clearSchedule: (id) => {
      const index = scheduled.findIndex((entry) => entry.id === id);
      if (index >= 0) scheduled.splice(index, 1);
    },
    raf: (fn) => {
      const id = nextRafId++;
      rafQueue.push({ id, fn });
      return id;
    },
    cancelRaf: (id) => {
      const index = rafQueue.findIndex((entry) => entry.id === id);
      if (index >= 0) rafQueue.splice(index, 1);
    },
  });

  const advanceTime = (ms: number) => {
    now += ms;
    const due = scheduled.filter((entry) => entry.fireAt <= now);
    for (const entry of due) {
      const index = scheduled.indexOf(entry);
      if (index >= 0) scheduled.splice(index, 1);
      entry.fn();
    }
  };

  const flushRaf = () => {
    const batch = rafQueue.splice(0, rafQueue.length);
    for (const entry of batch) entry.fn();
  };

  const flushMicrotasks = async () => {
    await new Promise<void>((resolve) => queueMicrotask(resolve));
  };

  return { policy, advanceTime, flushRaf, flushMicrotasks, scheduled, rafQueue };
};

describe("terminalLayoutPolicy", () => {
  it("keyboard closed → allowLocalFit and allowPtyResize true, cropToBottom false", () => {
    const { policy } = createTestHarness();
    policy.setKeyboardOpen(false);
    expect(policy.decision()).toEqual(allowed);
  });

  it("keyboard open with no discrete intent → both allows false, cropToBottom true", () => {
    const { policy } = createTestHarness();
    policy.setKeyboardOpen(true);
    expect(policy.decision()).toEqual(frozen);
  });

  it("keyboard open edge → pinToBottomOnKeyboardOpen true once, then false", () => {
    const { policy } = createTestHarness();
    policy.setKeyboardOpen(false);
    expect(policy.setKeyboardOpen(true)).toEqual({
      ...frozen,
      pinToBottomOnKeyboardOpen: true,
    });
    expect(policy.decision()).toEqual(frozen);
    expect(policy.setKeyboardOpen(true)).toEqual(frozen);
  });

  it("pinchEnded while keyboard open → allows true until double-rAF clear; then freeze again", async () => {
    const { policy, flushRaf, flushMicrotasks } = createTestHarness();
    policy.setKeyboardOpen(true);
    expect(policy.pinchEnded()).toEqual(allowed);
    expect(policy.decision()).toEqual(allowed);
    await flushMicrotasks();
    flushRaf();
    expect(policy.decision()).toEqual(allowed);
    flushRaf();
    expect(policy.decision()).toEqual(frozen);
  });

  it("expandEnter while keyboard open → allows true through EXPAND_REWRAP_MS + double-rAF clear", () => {
    const { policy, advanceTime, flushRaf } = createTestHarness();
    policy.setKeyboardOpen(true);
    expect(policy.expandEnter()).toEqual(allowed);
    expect(policy.decision()).toEqual(allowed);

    advanceTime(EXPAND_REWRAP_MS - 1);
    expect(policy.decision()).toEqual(allowed);

    advanceTime(1);
    expect(policy.decision()).toEqual(allowed);
    flushRaf();
    expect(policy.decision()).toEqual(allowed);
    flushRaf();
    expect(policy.decision()).toEqual(frozen);
  });

  it("expandExit does not leave a stale expand intent", () => {
    const { policy, advanceTime } = createTestHarness();
    policy.setKeyboardOpen(true);
    policy.expandEnter();
    expect(policy.decision()).toEqual(allowed);
    policy.expandExit();
    expect(policy.decision()).toEqual(frozen);

    advanceTime(EXPAND_REWRAP_MS + 100);
    expect(policy.decision()).toEqual(frozen);
  });

  it("dispose clears pending timers/intents; later ticks are no-ops", async () => {
    const { policy, advanceTime, flushRaf, flushMicrotasks, scheduled } = createTestHarness();
    policy.setKeyboardOpen(true);
    policy.expandEnter();
    policy.pinchEnded();
    await flushMicrotasks();
    expect(scheduled.length).toBeGreaterThan(0);

    policy.dispose();
    expect(scheduled).toHaveLength(0);
    expect(policy.decision()).toEqual(frozen);

    advanceTime(EXPAND_REWRAP_MS + 100);
    flushRaf();
    flushRaf();
    expect(policy.decision()).toEqual(frozen);
  });

  it("EXPAND_REWRAP_MS is 280", () => {
    expect(EXPAND_REWRAP_MS).toBe(280);
  });
});
