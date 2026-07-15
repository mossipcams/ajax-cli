import { describe, it, expect, vi, beforeEach } from "vitest";

const ghosttyLoad = vi.hoisted(() => vi.fn(() => Promise.resolve({ runtime: "ghostty" })));
const fetchMock = vi.hoisted(() => vi.fn(() => Promise.resolve({ ok: true })));
const preloadWtermGhosttyCore = vi.hoisted(() =>
  vi.fn(() => Promise.resolve({ runtime: "wterm-ghostty-core" })),
);

vi.mock("ghostty-web", () => ({
  Ghostty: {
    load: ghosttyLoad,
  },
}));

vi.mock("./components/TerminalRawView.svelte", () => ({
  default: {},
}));

vi.mock("./components/WtermTerminalView.svelte", () => ({
  default: {},
}));

vi.mock("./terminalWtermGhosttyCore", () => ({
  preloadWtermGhosttyCore,
}));

describe("terminalPreload", () => {
  beforeEach(() => {
    ghosttyLoad.mockClear();
    fetchMock.mockClear();
    preloadWtermGhosttyCore.mockClear();
    vi.stubGlobal("fetch", fetchMock);
    localStorage.clear();
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

  it("warmTerminalAssets preloads ghostty when Surface V2 is off", async () => {
    const { warmTerminalAssets } = await import("./terminalPreload");

    await warmTerminalAssets();

    expect(ghosttyLoad).toHaveBeenCalledTimes(1);
    expect(ghosttyLoad).toHaveBeenCalledWith("/ghostty-vt.wasm");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("warmTerminalAssets preloads the wterm component chunk when Surface V2 is on", async () => {
    localStorage.setItem("ajax.terminal.surfaceV2", "true");
    const { warmTerminalAssets } = await import("./terminalPreload");

    const results = await warmTerminalAssets();

    expect(ghosttyLoad).not.toHaveBeenCalled();
    expect(preloadWtermGhosttyCore).not.toHaveBeenCalled();
    expect(results).toHaveLength(1);
    expect(results[0]).toEqual({ default: {} });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
