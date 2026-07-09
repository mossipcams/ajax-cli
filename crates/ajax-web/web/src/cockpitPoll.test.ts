import { describe, it, expect, vi } from "vitest";
import cockpit from "./fixtures/cockpit.json";
import type { BrowserCockpitView } from "./types";
import {
  stableCockpitHash,
  createCockpitApplyGate,
  createInFlightGuard,
} from "./cockpitPoll";

const fixture = cockpit as BrowserCockpitView;

describe("stableCockpitHash", () => {
  it("is stable for deep-equal objects", () => {
    const copy = structuredClone(fixture);
    expect(stableCockpitHash(fixture)).toBe(stableCockpitHash(copy));
  });

  it("differs when a field changes", () => {
    const changed = structuredClone(fixture);
    changed.cards[0] = { ...changed.cards[0], title: "Changed title" };
    expect(stableCockpitHash(fixture)).not.toBe(stableCockpitHash(changed));
  });
});

describe("createCockpitApplyGate", () => {
  it("returns true on first apply and false when hash matches", () => {
    const gate = createCockpitApplyGate();
    expect(gate.applyIfChanged(fixture)).toBe(true);
    expect(gate.applyIfChanged(structuredClone(fixture))).toBe(false);
  });

  it("returns true again after payload changes", () => {
    const gate = createCockpitApplyGate();
    gate.applyIfChanged(fixture);
    const changed = structuredClone(fixture);
    changed.cards[0] = { ...changed.cards[0], title: "New title" };
    expect(gate.applyIfChanged(changed)).toBe(true);
    expect(gate.applyIfChanged(structuredClone(changed))).toBe(false);
  });

  it("reset clears the remembered hash", () => {
    const gate = createCockpitApplyGate();
    gate.applyIfChanged(fixture);
    gate.reset();
    expect(gate.applyIfChanged(structuredClone(fixture))).toBe(true);
  });
});

describe("createInFlightGuard", () => {
  it("skips a second run while the first is pending", async () => {
    const guard = createInFlightGuard();
    let fetchCount = 0;
    const fetch = vi.fn(async () => {
      fetchCount += 1;
      await new Promise((r) => setTimeout(r, 20));
      return "ok";
    });

    const first = guard.run(fetch);
    const second = guard.run(fetch);

    expect(fetch).toHaveBeenCalledTimes(1);
    expect(await second).toBeUndefined();
    expect(await first).toBe("ok");
    expect(fetchCount).toBe(1);
  });

  it("allows the next run after the first settles", async () => {
    const guard = createInFlightGuard();
    const fetch = vi.fn(async () => "done");

    await guard.run(fetch);
    const result = await guard.run(fetch);

    expect(fetch).toHaveBeenCalledTimes(2);
    expect(result).toBe("done");
  });
});
