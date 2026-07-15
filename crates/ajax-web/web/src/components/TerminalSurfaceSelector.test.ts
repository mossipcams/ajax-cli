import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import TerminalSurfaceSelector from "./TerminalSurfaceSelector.svelte";
import * as setting from "../terminalSurfaceSetting";

const ghosttyWebLoad = vi.hoisted(() => vi.fn(() => Promise.resolve({ runtime: "ghostty" })));
const termOpen = vi.hoisted(() => vi.fn());
const termDispose = vi.hoisted(() => vi.fn());
let xtermCtorShouldThrow = false;

vi.mock("ghostty-web", () => ({
  Ghostty: { load: ghosttyWebLoad },
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    textarea = document.createElement("textarea");
    element = document.createElement("div");
    renderer = { getMetrics: () => ({ width: 8, height: 18 }) };
    buffer = { active: { viewportY: 0, baseY: 0, getLine: () => undefined } };
    loadAddon = vi.fn();
    open = vi.fn();
    write = vi.fn();
    dispose = vi.fn();
    focus = vi.fn();
    blur = vi.fn();
    reset = vi.fn();
    paste = vi.fn();
    resize = vi.fn();
    onData = vi.fn(() => ({ dispose: vi.fn() }));
    onScroll = vi.fn(() => ({ dispose: vi.fn() }));
    scrollToBottom = vi.fn();
    scrollLines = vi.fn();
    getViewportY = vi.fn(() => 0);
    getSelection = vi.fn(() => "");
    clearSelection = vi.fn();
    getScrollbackLength = vi.fn(() => 0);
    attachCustomKeyEventHandler = vi.fn();
    selectionManager = { selectionStart: null, selectionEnd: null, requestRender: vi.fn() };
    options = { fontSize: 13 };
  },
  FitAddon: class MockFitAddon {
    fit = vi.fn();
    dispose = vi.fn();
    proposeDimensions = vi.fn(() => ({ cols: 80, rows: 24 }));
  },
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: class MockXtermTerminal {
    cols = 80;
    rows = 24;
    constructor() {
      if (xtermCtorShouldThrow) {
        throw new Error("xterm init failed");
      }
    }
    loadAddon = vi.fn();
    open = termOpen;
    write = vi.fn();
    dispose = termDispose;
    onData = vi.fn(() => ({ dispose: vi.fn() }));
  },
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class MockXtermFitAddon {
    fit = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock("../terminalPreload", () => ({
  preloadGhosttyRuntime: vi.fn(() => Promise.resolve({ runtime: "ghostty" })),
}));

const connectionDispose = vi.fn();

vi.mock("../terminalConnection", () => ({
  connectTaskTerminal: vi.fn(() => ({
    isOpen: () => true,
    sendInput: vi.fn(),
    sendResize: vi.fn(),
    reconnectNow: vi.fn(),
    dispose: connectionDispose,
  })),
}));

let v2Enabled = false;
let settingListener: ((enabled: boolean) => void) | undefined;

beforeEach(() => {
  localStorage.clear();
  v2Enabled = false;
  settingListener = undefined;
  xtermCtorShouldThrow = false;
  connectionDispose.mockClear();
  ghosttyWebLoad.mockReset();
  ghosttyWebLoad.mockResolvedValue({ runtime: "ghostty" });
  termOpen.mockClear();
  termDispose.mockClear();

  vi.spyOn(setting, "isTerminalSurfaceV2Enabled").mockImplementation(() => v2Enabled);
  vi.spyOn(setting, "subscribeTerminalSurfaceV2").mockImplementation((listener) => {
    settingListener = listener;
    return () => {
      settingListener = undefined;
    };
  });
  vi.spyOn(setting, "setTerminalSurfaceV2Enabled").mockImplementation((enabled) => {
    v2Enabled = enabled;
    settingListener?.(enabled);
  });

  vi.stubGlobal("WebSocket", class {
    readyState = 1;
    close() {}
    addEventListener() {}
    send() {}
  });
  vi.stubGlobal(
    "ResizeObserver",
    class MockResizeObserver {
      observe = vi.fn();
      disconnect = vi.fn();
    },
  );
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    cb(0);
    return 0;
  });
});

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("TerminalSurfaceSelector", () => {
  it("defaults to Ghostty only", async () => {
    const { container } = render(TerminalSurfaceSelector, { props: { handle: "web/fix" } });
    await waitFor(() => {
      expect(container.querySelector('[data-terminal-engine="ghostty"]')).toBeTruthy();
      expect(container.querySelector("[data-terminal-engine]")?.getAttribute("data-terminal-engine")).toBe(
        "ghostty",
      );
    });
  });

  it("mounts xterm when Surface V2 is enabled", async () => {
    v2Enabled = true;
    const { container } = render(TerminalSurfaceSelector, {
      props: { handle: "web/fix" },
    });
    await waitFor(() => {
      expect(container.querySelector('[data-terminal-engine="xterm"]')).toBeTruthy();
      expect(container.querySelector('[data-terminal-engine="ghostty"]')).toBeNull();
      expect(ghosttyWebLoad).not.toHaveBeenCalled();
    });
  });

  it("switching unmounts the previous surface connection", async () => {
    const { unmount } = render(TerminalSurfaceSelector, { props: { handle: "web/fix" } });
    await waitFor(() =>
      expect(document.querySelector('[data-terminal-engine="ghostty"]')).toBeTruthy(),
    );
    const disposesBefore = connectionDispose.mock.calls.length;
    v2Enabled = true;
    settingListener?.(true);
    await tick();
    await waitFor(() => {
      expect(connectionDispose.mock.calls.length).toBeGreaterThan(disposesBefore);
      expect(document.querySelector('[data-terminal-engine="xterm"]')).toBeTruthy();
    });
    unmount();
  });

  it("never mounts both surfaces at once", async () => {
    v2Enabled = true;
    const { container } = render(TerminalSurfaceSelector, { props: { handle: "web/fix" } });
    await waitFor(() => {
      const engines = container.querySelectorAll("[data-terminal-engine]");
      expect(engines.length).toBe(1);
      expect(engines[0]?.getAttribute("data-terminal-engine")).toBe("xterm");
    });
  });

  it("shows error banner on xterm init failure without loading Ghostty", async () => {
    xtermCtorShouldThrow = true;
    v2Enabled = true;
    const { getByTestId } = render(TerminalSurfaceSelector, {
      props: { handle: "web/fix" },
    });
    await waitFor(() => {
      const errorEl = getByTestId("terminal-surface-v2-error");
      expect(errorEl.textContent).toMatch(/xterm init failed/i);
      expect(document.querySelector('[data-terminal-engine="xterm"]')).toBeNull();
      expect(ghosttyWebLoad).not.toHaveBeenCalled();
    });
  });

  it("retry remounts xterm after init failure", async () => {
    xtermCtorShouldThrow = true;
    v2Enabled = true;
    const { getByTestId, getByRole } = render(TerminalSurfaceSelector, {
      props: { handle: "web/fix" },
    });
    await waitFor(() => {
      expect(getByTestId("terminal-surface-v2-error").textContent).toMatch(/xterm init failed/i);
    });
    xtermCtorShouldThrow = false;
    await getByRole("button", { name: "Retry" }).click();
    await waitFor(() => {
      expect(document.querySelector('[data-terminal-engine="xterm"]')).toBeTruthy();
      expect(document.querySelector('[data-terminal-engine="ghostty"]')).toBeNull();
      expect(ghosttyWebLoad).not.toHaveBeenCalled();
    });
  });
});
