import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  isTerminalSurfaceV2Enabled,
  setTerminalSurfaceV2Enabled,
  subscribeTerminalSurfaceV2,
} from "./terminalSurfaceSetting";

const STORAGE_KEY = "ajax.terminal.surfaceV2";

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  localStorage.clear();
});

describe("terminalSurfaceSetting", () => {
  it("defaults to off", () => {
    expect(isTerminalSurfaceV2Enabled()).toBe(false);
  });

  it("persists true and false across get/set", () => {
    setTerminalSurfaceV2Enabled(true);
    expect(localStorage.getItem(STORAGE_KEY)).toBe("true");
    expect(isTerminalSurfaceV2Enabled()).toBe(true);

    setTerminalSurfaceV2Enabled(false);
    expect(localStorage.getItem(STORAGE_KEY)).toBe("false");
    expect(isTerminalSurfaceV2Enabled()).toBe(false);
  });

  it("subscribe notifies on change", () => {
    const values: boolean[] = [];
    const unsubscribe = subscribeTerminalSurfaceV2((enabled) => values.push(enabled));

    setTerminalSurfaceV2Enabled(true);
    setTerminalSurfaceV2Enabled(false);

    unsubscribe();
    setTerminalSurfaceV2Enabled(true);

    expect(values).toEqual([true, false]);
  });
});
