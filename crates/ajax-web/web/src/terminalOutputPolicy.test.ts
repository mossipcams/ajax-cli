import { describe, it, expect } from "vitest";
import {
  scrollbackGrowthCompensation,
  outputFollowEffects,
  validTerminalSize,
} from "./terminalOutputPolicy";

describe("terminalOutputPolicy", () => {
  it("compensates positive scrollback growth while preserving reader position", () => {
    expect(scrollbackGrowthCompensation(40, 42)).toBe(-2);
    expect(scrollbackGrowthCompensation(40, 40)).toBe(0);
    expect(scrollbackGrowthCompensation(42, 40)).toBe(0);
    expect(scrollbackGrowthCompensation(NaN, 42)).toBe(0);
    expect(scrollbackGrowthCompensation(40, NaN)).toBe(0);
    expect(scrollbackGrowthCompensation(Infinity, 42)).toBe(0);
    expect(scrollbackGrowthCompensation(40, Infinity)).toBe(0);
  });

  it("maps pinned state to output follow effects", () => {
    expect(outputFollowEffects(true)).toEqual({
      snapToBottom: true,
      markUnseenOutput: false,
    });
    expect(outputFollowEffects(false)).toEqual({
      snapToBottom: false,
      markUnseenOutput: true,
    });
  });

  it("accepts only finite positive integer resize sizes", () => {
    expect(validTerminalSize(80, 24)).toEqual({ cols: 80, rows: 24 });
    expect(validTerminalSize(NaN, 24)).toBeUndefined();
    expect(validTerminalSize(80, NaN)).toBeUndefined();
    expect(validTerminalSize(Infinity, 24)).toBeUndefined();
    expect(validTerminalSize(80, Infinity)).toBeUndefined();
    expect(validTerminalSize(0, 24)).toBeUndefined();
    expect(validTerminalSize(80, 0)).toBeUndefined();
    expect(validTerminalSize(-1, 24)).toBeUndefined();
    expect(validTerminalSize(80, -1)).toBeUndefined();
    expect(validTerminalSize(80.5, 24)).toBeUndefined();
    expect(validTerminalSize(80, 24.5)).toBeUndefined();
  });
});
