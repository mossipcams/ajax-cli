import { describe, it, expect } from "vitest";
import {
  pullStart,
  pullMove,
  pullEnd,
  PULL_THRESHOLD,
  PULL_MAX,
} from "./pullToRefresh";

describe("pull-to-refresh gesture", () => {
  it("only activates when the scroll container is at the top", () => {
    expect(pullStart(0).active).toBe(true);
    expect(pullStart(12).active).toBe(false);
  });

  it("applies rubber-band resistance and caps the distance", () => {
    const state = pullMove(pullStart(0), 100);
    expect(state.distance).toBeGreaterThan(0);
    expect(state.distance).toBeLessThan(100); // resisted
    expect(pullMove(pullStart(0), 1000).distance).toBe(PULL_MAX);
  });

  it("arms once the resisted distance passes the threshold", () => {
    expect(pullMove(pullStart(0), 40).armed).toBe(false);
    expect(pullMove(pullStart(0), 1000).armed).toBe(true);
    expect(pullMove(pullStart(0), 1000).distance).toBeGreaterThanOrEqual(PULL_THRESHOLD);
  });

  it("ignores upward movement and never arms", () => {
    const state = pullMove(pullStart(0), -50);
    expect(state.distance).toBe(0);
    expect(state.armed).toBe(false);
  });

  it("triggers on release only when armed", () => {
    expect(pullEnd(pullMove(pullStart(0), 1000)).triggered).toBe(true);
    expect(pullEnd(pullMove(pullStart(0), 20)).triggered).toBe(false);
    expect(pullEnd(pullMove(pullStart(40), 1000)).triggered).toBe(false); // not at top
  });
});
