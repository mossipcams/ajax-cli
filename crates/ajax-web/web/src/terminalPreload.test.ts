import { describe, it, expect, vi, beforeEach } from "vitest";

const ghosttyLoad = vi.hoisted(() => vi.fn(() => Promise.resolve({ runtime: "ghostty" })));

vi.mock("ghostty-web", () => ({
  Ghostty: {
    load: ghosttyLoad,
  },
}));

const terminalViewImport = vi.hoisted(() => vi.fn(() => Promise.resolve({ default: {} })));

vi.mock("./components/TerminalRawView.svelte", () => ({
  default: {},
}));

describe("terminalPreload", () => {
  beforeEach(() => {
    ghosttyLoad.mockClear();
    terminalViewImport.mockClear();
    vi.resetModules();
  });

  it("loads ghostty wasm once when preloadGhosttyRuntime is called repeatedly", async () => {
    const { preloadGhosttyRuntime, GHOSTTY_WASM_URL } = await import("./terminalPreload");

    expect(GHOSTTY_WASM_URL).toBe("/ghostty-vt.wasm");

    await Promise.all([preloadGhosttyRuntime(), preloadGhosttyRuntime()]);

    expect(ghosttyLoad).toHaveBeenCalledTimes(1);
    expect(ghosttyLoad).toHaveBeenCalledWith("/ghostty-vt.wasm");
  });

  it("preloads the terminal view chunk", async () => {
    const { preloadTerminalView } = await import("./terminalPreload");
    await expect(preloadTerminalView()).resolves.toBeDefined();
  });

  it("warmTerminalAssets preloads ghostty and the terminal view chunk", async () => {
    const { warmTerminalAssets } = await import("./terminalPreload");

    await warmTerminalAssets();

    expect(ghosttyLoad).toHaveBeenCalledTimes(1);
    expect(ghosttyLoad).toHaveBeenCalledWith("/ghostty-vt.wasm");
  });
});
