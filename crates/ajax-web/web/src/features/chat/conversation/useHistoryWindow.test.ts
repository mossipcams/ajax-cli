import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { ConversationItem } from "../session/public";
import { DEFAULT_HISTORY_WINDOW, HISTORY_REVEAL_BATCH, snapRevealStart } from "./historyWindow";
import { useHistoryWindow } from "./useHistoryWindow";

const userProse = (id: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "user",
  text: id,
});

const agentProse = (id: string): ConversationItem => ({
  kind: "prose",
  id,
  role: "agent",
  text: id,
});

function buildLongTranscript(count: number): ConversationItem[] {
  const items: ConversationItem[] = [];
  for (let turn = 0; turn < count; turn += 1) {
    items.push(userProse(`u${turn}`), agentProse(`a${turn}`));
  }
  return items;
}

describe("useHistoryWindow", () => {
  it("paints only the recent window on first populate", () => {
    const items = buildLongTranscript(120);
    const { result } = renderHook(({ rows, key }) => useHistoryWindow(rows, key), {
      initialProps: { rows: items, key: "task:ready" },
    });

    expect(result.current.visibleItems.length).toBeLessThanOrEqual(DEFAULT_HISTORY_WINDOW);
    expect(result.current.visibleItems[0]?.id).toBe(
      items[items.length - result.current.visibleItems.length]?.id,
    );
    expect(result.current.hasEarlier).toBe(true);
  });

  it("derives the window slice during first render before layout effects", () => {
    const items = buildLongTranscript(120);
    let firstRenderVisible = -1;
    renderHook(({ rows, key }) => {
      const hook = useHistoryWindow(rows, key);
      if (firstRenderVisible < 0) {
        firstRenderVisible = hook.visibleItems.length;
      }
      return hook;
    }, {
      initialProps: { rows: items, key: "task:ready" },
    });

    expect(firstRenderVisible).toBeLessThanOrEqual(DEFAULT_HISTORY_WINDOW);
    expect(firstRenderVisible).toBeGreaterThan(0);
    expect(firstRenderVisible).toBeLessThan(items.length);
  });

  it("grows the visible tail without moving the start index", () => {
    const initial = buildLongTranscript(100);
    const { result, rerender } = renderHook(({ rows, key }) => useHistoryWindow(rows, key), {
      initialProps: { rows: initial, key: "task:ready" },
    });
    const start = result.current.windowStart;

    rerender({ rows: [...initial, userProse("u-new"), agentProse("a-new")], key: "task:ready" });

    expect(result.current.windowStart).toBe(start);
    expect(result.current.visibleItems.at(-1)?.id).toBe("a-new");
  });

  it("resets when the session key changes", () => {
    const items = buildLongTranscript(100);
    const { result, rerender } = renderHook(({ rows, key }) => useHistoryWindow(rows, key), {
      initialProps: { rows: items, key: "task-a:ready" },
    });
    const initialStart = result.current.windowStart;
    act(() => {
      result.current.revealEarlier();
    });
    expect(result.current.windowStart).toBeLessThan(initialStart);

    rerender({ rows: items, key: "task-b:ready" });
    expect(result.current.windowStart).toBe(initialStart);
    expect(result.current.visibleItems.length).toBeLessThanOrEqual(DEFAULT_HISTORY_WINDOW);
  });

  it("reveals earlier already-held rows", () => {
    const items = buildLongTranscript(120);
    const { result } = renderHook(({ rows, key }) => useHistoryWindow(rows, key), {
      initialProps: { rows: items, key: "task:ready" },
    });
    const before = result.current.windowStart;

    act(() => {
      result.current.revealEarlier();
    });

    expect(result.current.windowStart).toBeLessThan(before);
    expect(result.current.visibleItems.length).toBeGreaterThan(DEFAULT_HISTORY_WINDOW - 50);
  });

  it("recomputes the window after shrink and replay on the same session key", () => {
    const items = buildLongTranscript(120);
    const { result, rerender } = renderHook(({ rows, key }) => useHistoryWindow(rows, key), {
      initialProps: { rows: items, key: "task:ready" },
    });
    const initialStart = result.current.windowStart;
    expect(result.current.visibleItems.length).toBeLessThanOrEqual(DEFAULT_HISTORY_WINDOW);

    act(() => {
      result.current.revealEarlier();
    });
    expect(result.current.windowStart).toBeLessThan(initialStart);

    rerender({ rows: [], key: "task:ready" });
    expect(result.current.visibleItems).toEqual([]);
    expect(result.current.windowStart).toBe(0);

    rerender({ rows: items, key: "task:ready" });
    expect(result.current.windowStart).toBe(initialStart);
    expect(result.current.visibleItems.length).toBeLessThanOrEqual(DEFAULT_HISTORY_WINDOW);
    expect(result.current.visibleItems[0]?.id).toBe(
      items[items.length - result.current.visibleItems.length]?.id,
    );
  });
});


describe("useHistoryWindow server prepends", () => {
  it("bounds the first prepend render to one batch snapped to a user turn", () => {
    const current = [userProse("current-user"), agentProse("current-agent")];
    const older = [...buildLongTranscript(60), userProse("older-last")];
    const renders: number[] = [];
    const { result, rerender } = renderHook(({ rows, from }) => {
      const window = useHistoryWindow(rows, "task", from);
      renders.push(window.visibleItems.length);
      return window;
    }, { initialProps: { rows: current, from: 200 } });
    renders.length = 0;
    const rows = [...older, ...current];
    rerender({ rows, from: 20 });
    const expectedStart = snapRevealStart(rows, older.length, HISTORY_REVEAL_BATCH);
    expect(result.current.windowStart).toBe(expectedStart);
    expect(result.current.visibleItems[0]).toMatchObject({ kind: "prose", role: "user" });
    expect(result.current.visibleItems.slice(-current.length)).toEqual(current);
    expect(result.current.windowGeneration).toBe(1);
    expect(renders.every((count) => count <= current.length + HISTORY_REVEAL_BATCH)).toBe(true);
    expect(result.current.hasEarlier).toBe(true);
    act(() => expect(result.current.revealEarlier()).toBeGreaterThan(0));
    expect(result.current.windowStart).toBeLessThan(expectedStart);
  });

  it("bounds a server prepend into an empty loaded transcript", () => {
    const { result, rerender } = renderHook(({ rows, from }) => useHistoryWindow(rows, "task", from), {
      initialProps: { rows: [] as ConversationItem[], from: 200 },
    });
    const rows = buildLongTranscript(120);
    rerender({ rows, from: 0 });
    expect(result.current.visibleItems).toEqual(rows.slice(-HISTORY_REVEAL_BATCH));
    expect(result.current.hasEarlier).toBe(true);
  });

  it("preserves live tail growth when a server page is prepended", () => {
    const current = [userProse("current-user"), agentProse("current-agent")];
    const { result, rerender } = renderHook(({ rows, from }) => useHistoryWindow(rows, "task", from), {
      initialProps: { rows: current, from: 200 },
    });
    const live = [userProse("live-user"), agentProse("live-agent")];
    rerender({ rows: [...current, ...live], from: 200 });
    const older = buildLongTranscript(100);
    rerender({ rows: [...older, ...current, ...live], from: 0 });
    expect(result.current.visibleItems.length).toBe(HISTORY_REVEAL_BATCH + current.length + live.length);
    expect(result.current.visibleItems.slice(-4)).toEqual([...current, ...live]);
  });

  it("resets normally when a different session has a lower history bound", () => {
    const current = [userProse("current-user")];
    const { result, rerender } = renderHook(({ rows, from, key }) => useHistoryWindow(rows, key, from), {
      initialProps: { rows: current, from: 200, key: "task-a" },
    });
    const rows = [...buildLongTranscript(100), ...current];
    rerender({ rows, from: 0, key: "task-b" });
    expect(result.current.visibleItems.length).toBeGreaterThan(HISTORY_REVEAL_BATCH);
    expect(result.current.visibleItems.length).toBeLessThanOrEqual(DEFAULT_HISTORY_WINDOW);
    expect(result.current.windowGeneration).toBe(0);
  });
});
