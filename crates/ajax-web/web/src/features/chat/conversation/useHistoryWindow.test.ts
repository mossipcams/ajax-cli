import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { ConversationItem } from "../session/public";
import { DEFAULT_HISTORY_WINDOW } from "./historyWindow";
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
