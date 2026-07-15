import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import WtermTerminalView from "./WtermTerminalView.svelte";
import wtermTerminalViewSource from "./WtermTerminalView.svelte?raw";
import { RESIZE_DEBOUNCE_MS } from "../terminalRefit";

const termWrite = vi.fn();
const termFocus = vi.fn();
const termDestroy = vi.fn();
const termResize = vi.fn();
let termOnData: ((data: string) => void) | undefined;
let termOnResize: ((cols: number, rows: number) => void) | undefined;
let lastWterm: { cols: number; rows: number } | undefined;
let lastWtermOptions:
  | {
      core?: unknown;
      cols?: number;
      rows?: number;
      autoResize?: boolean;
      onData?: (data: string) => void;
      onResize?: (cols: number, rows: number) => void;
    }
  | undefined;

const vvListeners: Record<string, Array<() => void>> = {};

function dispatchVisualViewport(type: string) {
  for (const handler of vvListeners[type] ?? []) handler();
}

const coreBracketedPaste = vi.hoisted(() => vi.fn(() => false));
const coreCursorKeysApp = vi.hoisted(() => vi.fn(() => false));
const wasmBridgeLoad = vi.hoisted(() =>
  vi.fn(() =>
    Promise.resolve({
      bracketedPaste: coreBracketedPaste,
      cursorKeysApp: coreCursorKeysApp,
    }),
  ),
);

vi.mock("@wterm/core", () => ({
  WasmBridge: {
    load: wasmBridgeLoad,
  },
}));

vi.mock("@wterm/dom", () => ({
  WTerm: class MockWTerm {
    cols = 80;
    rows = 24;
    constructor(
      _el: HTMLElement,
      options?: {
        core?: unknown;
        cols?: number;
        rows?: number;
        autoResize?: boolean;
        onData?: (data: string) => void;
        onResize?: (cols: number, rows: number) => void;
      },
    ) {
      lastWtermOptions = options;
      termOnData = options?.onData;
      termOnResize = options?.onResize;
      if (options?.cols !== undefined) this.cols = options.cols;
      if (options?.rows !== undefined) this.rows = options.rows;
      lastWterm = this;
    }
    init = vi.fn(() => Promise.resolve(this));
    resize = (...args: [number?, number?]) => {
      if (args[0] !== undefined) this.cols = args[0];
      if (args[1] !== undefined) this.rows = args[1];
      termResize(this.cols, this.rows);
    };
    write = termWrite;
    focus = termFocus;
    destroy = termDestroy;
  },
}));

const sendInput = vi.fn();
const sendResize = vi.fn();
const dispose = vi.fn();
const reconnectNow = vi.fn();
let connectionOpen = true;
let connectionEvents:
  | {
      onOutput: (text: string) => void;
      onServerError: (message: string) => void;
      onStatus: (status: string) => void;
      onOpen: () => void;
    }
  | undefined;
let onOutput: ((text: string) => void) | undefined;

vi.mock("../terminalConnection", () => ({
  connectTaskTerminal: vi.fn(
    (
      _handle: string,
      events: {
        onOutput: (text: string) => void;
        onServerError: (message: string) => void;
        onStatus: (status: string) => void;
        onOpen: () => void;
      },
    ) => {
      connectionEvents = events;
      onOutput = events.onOutput;
      return {
        isOpen: () => connectionOpen,
        sendInput,
        sendResize,
        reconnectNow,
        dispose,
      };
    },
  ),
}));

beforeEach(() => {
  document.documentElement.classList.remove("terminal-expanded", "keyboard-open");
  window.localStorage.clear();
  for (const key of Object.keys(vvListeners)) delete vvListeners[key];
  lastWterm = undefined;
  lastWtermOptions = undefined;
  vi.clearAllMocks();
  // clearAllMocks keeps mockReturnValue overrides; re-pin the core-mode
  // defaults so a test enabling a mode cannot leak into later tests.
  coreBracketedPaste.mockReturnValue(false);
  coreCursorKeysApp.mockReturnValue(false);
  connectionOpen = true;
  connectionEvents = undefined;
  onOutput = undefined;
  termOnData = undefined;
  termOnResize = undefined;
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
  vi.stubGlobal("visualViewport", {
    addEventListener: (type: string, handler: () => void) => {
      (vvListeners[type] ??= []).push(handler);
    },
    removeEventListener: vi.fn(),
  });
});

afterEach(() => vi.restoreAllMocks());

describe("WtermTerminalView", () => {
  it('exposes data-terminal-engine="wterm"', async () => {
    const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => {
      expect(getByTestId("task-terminal-panel").getAttribute("data-terminal-engine")).toBe("wterm");
    });
  });

  it("loads the built-in WasmBridge core with a fixed 80x24 grid", async () => {
    render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(wasmBridgeLoad).toHaveBeenCalledTimes(1));
    expect(lastWtermOptions).toMatchObject({
      cols: 80,
      rows: 24,
      autoResize: false,
    });
    expect(lastWtermOptions?.core).toEqual(
      expect.objectContaining({
        bracketedPaste: coreBracketedPaste,
        cursorKeysApp: coreCursorKeysApp,
      }),
    );
  });

  it("routes PTY output from the connection callback to term.write", async () => {
    render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(onOutput).toBeTypeOf("function"));
    onOutput!("hello");
    expect(termWrite).toHaveBeenCalledWith("hello");
  });

  it("routes onData to connection.sendInput", async () => {
    render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(termOnData).toBeTypeOf("function"));
    termOnData!("a");
    expect(sendInput).toHaveBeenCalledWith("a");
  });

  it("routes resize through connection.sendResize with actual cols and rows", async () => {
    render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(termOnResize).toBeTypeOf("function"));
    sendResize.mockClear();
    termOnResize!(40, 20);
    expect(sendResize).toHaveBeenLastCalledWith(40, 20);
  });

  it("clamps tiny resize dimensions to at least one col and row", async () => {
    render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(termOnResize).toBeTypeOf("function"));
    sendResize.mockClear();
    termOnResize!(0, 0);
    expect(sendResize).toHaveBeenLastCalledWith(1, 1);
  });

  it("does not force-fit with hardcoded 8×17 metrics after init", async () => {
    let resolveLoad: (value: { runtime: string }) => void = () => {};
    const loadPromise = new Promise<{ runtime: string }>((resolve) => {
      resolveLoad = resolve;
    });
    wasmBridgeLoad.mockImplementationOnce(() => loadPromise);

    const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
    const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
    Object.defineProperty(host, "clientWidth", { configurable: true, value: 320 });
    Object.defineProperty(host, "clientHeight", { configurable: true, value: 170 });
    resolveLoad({ runtime: "wasm-core" });

    await waitFor(() => expect(wasmBridgeLoad).toHaveBeenCalled());
    await tick();
    expect(termResize).not.toHaveBeenCalledWith(40, 10);
  });

  it("unmount calls connection.dispose and term.destroy", async () => {
    const { unmount } = render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(sendResize).toHaveBeenCalled());
    unmount();
    expect(dispose).toHaveBeenCalled();
    expect(termDestroy).toHaveBeenCalled();
  });

  it("init failure invokes onInitFailure and does not leave a live connection", async () => {
    wasmBridgeLoad.mockRejectedValueOnce(new Error("wasm missing"));
    const onInitFailure = vi.fn();
    render(WtermTerminalView, { props: { handle: "web/fix", onInitFailure } });
    await waitFor(() => expect(onInitFailure).toHaveBeenCalledWith("wasm missing"));
    expect(dispose).not.toHaveBeenCalled();
    await tick();
    expect(sendInput).not.toHaveBeenCalled();
  });
});

/**
 * Ghostty parity — behavioral contract derived from TerminalRawView.test.ts,
 * adapted to what wterm provides natively rather than ported one-to-one.
 *
 * Real tests here cover the Ajax-owned chrome (key bar, Ctrl arm, status,
 * paste button, reconnect). Terminal-core behaviors that Ghostty needed
 * Ajax-side modules for — scroll-follow, snap-on-type, VT response pump,
 * app-cursor arrows, bracketed paste, iOS-safe hidden input — are native to
 * wterm and pinned with the real WASM in
 * terminalWtermGhosttyCore.integration.test.ts, not re-mocked here.
 *
 * `it.todo` entries are the remaining executable parity checklist for
 * Ajax-side chrome gaps; wterm-native bracketed paste and DECCKM arrows are
 * covered by real tests in the parity-gaps describe block.
 *
 * Deliberately excluded (not gaps):
 * - zero-lag overlay + Ghostty selection-manager casts — TERMINAL.md marks
 *   them intentionally out of scope for the wterm surface
 * - WS reconnect backoff / visibility reconnect — shared terminalConnection.ts
 *   owns those for both surfaces and has its own tests
 */
describe("WtermTerminalView ghostty parity", () => {
  const mountWterm = async () => {
    const utils = render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(connectionEvents).toBeDefined());
    await waitFor(() => expect(termOnData).toBeTypeOf("function"));
    return utils;
  };

  it("sends the raw byte sequence for each control-bar key", async () => {
    const { getByRole } = await mountWterm();
    const expected: Array<[string, string]> = [
      ["Esc", "\x1b"],
      ["Tab", "\t"],
      ["⌃C", "\x03"],
      ["←", "\x1b[D"],
      ["↑", "\x1b[A"],
      ["↓", "\x1b[B"],
      ["→", "\x1b[C"],
    ];
    for (const [label, data] of expected) {
      sendInput.mockClear();
      await fireEvent.click(getByRole("button", { name: label }));
      expect(sendInput).toHaveBeenCalledWith(data);
    }
  });

  it("folds the next letter into a control code after Ctrl is armed", async () => {
    const { getByRole } = await mountWterm();

    await fireEvent.click(getByRole("button", { name: /Ctrl/ }));
    termOnData!("c");

    expect(sendInput).toHaveBeenCalledWith("\x03");
  });

  it("auto-disarms sticky Ctrl after the timeout so a later key is unmodified", async () => {
    const { getByRole } = await mountWterm();
    vi.useFakeTimers();
    try {
      await fireEvent.click(getByRole("button", { name: /Ctrl/ }));
      vi.advanceTimersByTime(4000);

      termOnData!("c");

      expect(sendInput).toHaveBeenCalledWith("c");
      expect(sendInput).not.toHaveBeenCalledWith("\x03");
    } finally {
      vi.useRealTimers();
    }
  });

  it("sends Enter unchanged when Ctrl is armed", async () => {
    const { getByRole } = await mountWterm();

    await fireEvent.click(getByRole("button", { name: /Ctrl/ }));
    termOnData!("\r");

    expect(sendInput).toHaveBeenCalledWith("\r");
  });

  it("applies an armed Ctrl to a control-bar cursor key, then disarms", async () => {
    const { getByRole } = await mountWterm();

    await fireEvent.click(getByRole("button", { name: /Ctrl/ }));
    await fireEvent.click(getByRole("button", { name: "←" }));

    expect(sendInput).toHaveBeenCalledWith("\x1b[1;5D");

    sendInput.mockClear();
    termOnData!("x");
    expect(sendInput).toHaveBeenCalledWith("x");
  });

  it("disarms Ctrl when tapped a second time and reflects the armed state", async () => {
    const { getByRole } = await mountWterm();
    const ctrl = getByRole("button", { name: /Ctrl/ });

    await fireEvent.click(ctrl);
    expect(ctrl.getAttribute("aria-pressed")).toBe("true");

    await fireEvent.click(ctrl);
    expect(ctrl.getAttribute("aria-pressed")).toBe("false");

    termOnData!("c");
    expect(sendInput).toHaveBeenCalledWith("c");
    expect(sendInput).not.toHaveBeenCalledWith("\x03");
  });

  it("offers a Hide keyboard key that blurs the terminal", async () => {
    const { getByRole } = await mountWterm();
    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();
    expect(document.activeElement).toBe(input);

    await fireEvent.click(getByRole("button", { name: "Hide keyboard" }));

    expect(document.activeElement).not.toBe(input);
    input.remove();
  });

  it("pastes clipboard text through the terminal input path", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: vi.fn().mockResolvedValue("git push origin main") },
      configurable: true,
    });
    const { getByRole } = await mountWterm();

    await fireEvent.click(getByRole("button", { name: "Paste" }));

    await waitFor(() => {
      expect(sendInput).toHaveBeenCalledWith("git push origin main");
    });
  });

  it("keeps a server error visible after a successful paste", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: vi.fn().mockResolvedValue("ls") },
      configurable: true,
    });
    const { getByRole, getByTestId } = await mountWterm();
    connectionEvents!.onServerError("tmux session missing");
    await waitFor(() => {
      expect(getByTestId("terminal-status").textContent).toContain("tmux session missing");
    });

    await fireEvent.click(getByRole("button", { name: "Paste" }));

    await waitFor(() => expect(sendInput).toHaveBeenCalledWith("ls"));
    expect(getByTestId("terminal-status").textContent).toContain("tmux session missing");
  });

  it("shows connection status transitions and hides the bar once connected", async () => {
    const { getByTestId } = await mountWterm();
    const statusEl = getByTestId("terminal-status");
    expect(statusEl.textContent).toContain("Connecting…");

    connectionEvents!.onStatus("reconnecting");
    await waitFor(() => expect(statusEl.textContent).toContain("Reconnecting…"));

    connectionEvents!.onStatus("connected");
    await waitFor(() => expect(statusEl.getAttribute("aria-hidden")).toBe("true"));
  });

  it("offers a manual reconnect button while reconnecting or unavailable", async () => {
    const { getByRole } = await mountWterm();

    connectionEvents!.onStatus("reconnecting");
    await waitFor(() => getByRole("button", { name: "Reconnect" }));
    await fireEvent.click(getByRole("button", { name: "Reconnect" }));
    expect(reconnectNow).toHaveBeenCalled();

    connectionEvents!.onStatus("unavailable");
    await waitFor(() => getByRole("button", { name: "Reconnect" }));
  });

  it("focuses the terminal and clears the server error when the socket opens", async () => {
    const { getByTestId } = await mountWterm();
    connectionEvents!.onServerError("bridge hiccup");
    await waitFor(() => {
      expect(getByTestId("terminal-status").textContent).toContain("bridge hiccup");
    });
    termFocus.mockClear();

    connectionEvents!.onOpen();

    await waitFor(() => expect(termFocus).toHaveBeenCalled());
    expect(getByTestId("terminal-status").textContent).not.toContain("bridge hiccup");
  });

  it("does not send key-bar input while the connection is closed", async () => {
    const { getByRole } = await mountWterm();
    connectionOpen = false;
    sendInput.mockClear();

    await fireEvent.click(getByRole("button", { name: "Esc" }));

    expect(sendInput).not.toHaveBeenCalled();
  });

  describe("parity gaps: key-bar focus discipline", () => {
    it("does not focus the terminal from a key-bar key, so a closed keyboard stays closed", async () => {
      const { getByRole } = await mountWterm();
      (document.activeElement as HTMLElement | null)?.blur();
      termFocus.mockClear();

      await fireEvent.click(getByRole("button", { name: "←" }));

      expect(sendInput).toHaveBeenCalledWith("\x1b[D"); // key still sends
      expect(termFocus).not.toHaveBeenCalled();
    });

    it("refocuses without scrolling when a key-bar key is tapped mid-typing", async () => {
      const { getByRole, getByTestId } = await mountWterm();
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      const input = document.createElement("input"); // stands in for wterm's hidden textarea
      host.appendChild(input);
      input.focus();
      termFocus.mockClear();

      await fireEvent.click(getByRole("button", { name: "→" }));

      expect(termFocus).toHaveBeenCalled(); // WTerm.focus() uses preventScroll internally
      input.remove();
    });
  });

  describe("parity gaps: use wterm-native capabilities the component bypasses", () => {
    it("routes the Paste key through wterm's bracketed-paste path (mode wrap + ESC strip)", async () => {
      Object.defineProperty(navigator, "clipboard", {
        value: { readText: vi.fn().mockResolvedValue("rm -rf\x1b[201~evil") },
        configurable: true,
      });
      coreBracketedPaste.mockReturnValue(true);
      const { getByRole } = await mountWterm();

      await fireEvent.click(getByRole("button", { name: "Paste" }));

      await waitFor(() => {
        expect(sendInput).toHaveBeenCalledWith("\x1b[200~rm -rf[201~evil\x1b[201~");
      });
    });

    it("key-bar arrows honor DECCKM application cursor mode via core.cursorKeysApp()", async () => {
      coreCursorKeysApp.mockReturnValue(true);
      const { getByRole } = await mountWterm();

      await fireEvent.click(getByRole("button", { name: "↑" }));
      expect(sendInput).toHaveBeenCalledWith("\x1bOA");

      await fireEvent.click(getByRole("button", { name: /Ctrl/ }));
      await fireEvent.click(getByRole("button", { name: "←" }));
      expect(sendInput).toHaveBeenCalledWith("\x1b[1;5D");
      expect(sendInput).not.toHaveBeenCalledWith("\x1bOD");
    });
    it("shows a New output control while the user is scrolled away from bottom", async () => {
      const { getByTestId, queryByRole, getByRole } = await mountWterm();
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      Object.defineProperty(host, "scrollHeight", { configurable: true, value: 340 });
      Object.defineProperty(host, "clientHeight", { configurable: true, value: 170 });

      // Pinned at bottom: output arrives, no affordance.
      host.scrollTop = 170;
      await fireEvent.scroll(host);
      onOutput!("at-bottom output");
      await tick();
      expect(queryByRole("button", { name: "New output ↓" })).toBeNull();

      // Scrolled up: output arrives, affordance appears.
      host.scrollTop = 0;
      await fireEvent.scroll(host);
      onOutput!("background update");
      const button = await waitFor(() => getByRole("button", { name: "New output ↓" }));

      // Tapping is a reading action: snap to bottom, hide, never focus.
      termFocus.mockClear();
      await fireEvent.click(button);
      expect(host.scrollTop).toBe(340);
      expect(termFocus).not.toHaveBeenCalled();
      expect(queryByRole("button", { name: "New output ↓" })).toBeNull();
    });
  });

  describe("parity gaps: geometry and font", () => {
    it("fits the initial grid with wterm-measured cell metrics instead of the hardcoded 8x17 estimate", async () => {
      let resolveLoad: (value: { runtime: string }) => void = () => {};
      const loadPromise = new Promise<{ runtime: string }>((resolve) => {
        resolveLoad = resolve;
      });
      wasmBridgeLoad.mockImplementationOnce(() => loadPromise);

      const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      Object.defineProperty(host, "clientWidth", { configurable: true, value: 320 });
      Object.defineProperty(host, "clientHeight", { configurable: true, value: 170 });
      resolveLoad({ runtime: "wasm-core" });

      await waitFor(() => expect(wasmBridgeLoad).toHaveBeenCalled());
      await tick();
      expect(termResize).not.toHaveBeenCalledWith(40, 10);
    });

    it("sets --term-font-size to 13px on the host element", async () => {
      const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
      await waitFor(() => expect(wasmBridgeLoad).toHaveBeenCalled());
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      expect(host.style.getPropertyValue("--term-font-size")).toBe("13px");
    });

    it("uses cooler #1e1e1e terminal chrome instead of warm #1c1714", async () => {
      const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
      await waitFor(() => expect(wasmBridgeLoad).toHaveBeenCalled());
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      expect(host.style.getPropertyValue("--term-bg")).toBe("#1e1e1e");
      expect(host.style.getPropertyValue("--term-bg")).not.toBe("#1c1714");
    });
    it.todo("uses agent-sized floor of 80 columns on a narrow host");
    // Readable/compact font: covered by --term-font-size 13px (DEFAULT_FONT_SIZE) above.
    // Horizontal pan: N/A on wterm DOM — native overflow scroll replaces Ghostty canvas pan.
    it("applies a persisted font size on mount", async () => {
      window.localStorage.setItem("ajax.terminal.fontSize", "16");
      const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
      await waitFor(() => expect(wasmBridgeLoad).toHaveBeenCalled());
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      expect(host.style.getPropertyValue("--term-font-size")).toBe("16px");
    });

    it("ignores an out-of-range persisted font size and uses the default", async () => {
      window.localStorage.setItem("ajax.terminal.fontSize", "999");
      const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
      await waitFor(() => expect(wasmBridgeLoad).toHaveBeenCalled());
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      expect(host.style.getPropertyValue("--term-font-size")).toBe("13px");
    });
    function makePinch(type: string, points: Array<{ x: number; y: number }>): Event {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", {
        value: points.map((point) => ({ clientX: point.x, clientY: point.y })),
      });
      return event;
    }

    it("grows/shrinks the font on pinch with clamps, persisting the choice", async () => {
      const { getByTestId } = await mountWterm();
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;

      // Default 13px; fingers spread 100px → 150px: 13 × 1.5 = 19.5 → 20 (clamp ceiling).
      host.dispatchEvent(
        makePinch("touchstart", [
          { x: 100, y: 100 },
          { x: 200, y: 100 },
        ]),
      );
      const growMove = makePinch("touchmove", [
        { x: 75, y: 100 },
        { x: 225, y: 100 },
      ]);
      host.dispatchEvent(growMove);

      expect(host.style.getPropertyValue("--term-font-size")).toBe("20px");
      expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBe("20");
      expect(growMove.defaultPrevented).toBe(true);

      // Pinch-in from 20px; 150px → 100px: 20 × (100/150) ≈ 13.
      host.dispatchEvent(
        makePinch("touchstart", [
          { x: 125, y: 100 },
          { x: 275, y: 100 },
        ]),
      );
      const shrinkMove = makePinch("touchmove", [
        { x: 150, y: 100 },
        { x: 250, y: 100 },
      ]);
      host.dispatchEvent(shrinkMove);

      expect(host.style.getPropertyValue("--term-font-size")).toBe("13px");
      expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBe("13");
      expect(shrinkMove.defaultPrevented).toBe(true);
    });
  });

  describe("parity gaps: keyboard lockstep", () => {
    it("debounces server resize on visualViewport without rebuilding the local grid", async () => {
      await mountWterm();
      vi.useFakeTimers();
      try {
        vi.advanceTimersByTime(50);
        sendResize.mockClear();
        termResize.mockClear();

        dispatchVisualViewport("resize");
        dispatchVisualViewport("scroll");
        dispatchVisualViewport("resize");

        vi.advanceTimersByTime(20);
        // Local fit is a no-op — resize() would rebuild the renderer and reset scroll.
        expect(termResize).not.toHaveBeenCalled();
        expect(sendResize).not.toHaveBeenCalled();

        if (lastWterm) {
          lastWterm.cols = 90;
          lastWterm.rows = 28;
        }

        vi.advanceTimersByTime(RESIZE_DEBOUNCE_MS - 21);
        expect(sendResize).not.toHaveBeenCalled();

        vi.advanceTimersByTime(1);
        expect(sendResize).toHaveBeenCalledTimes(1);
        expect(sendResize).toHaveBeenCalledWith(90, 28);
        expect(termResize).not.toHaveBeenCalled();
      } finally {
        vi.useRealTimers();
      }
    });

    it("allows vertical scroll on the wterm host (scrollback container)", async () => {
      // jsdom does not apply Svelte scoped CSS; pin the contract in source.
      const { default: source } = await import("./WtermTerminalView.svelte?raw");
      expect(source).toMatch(/\.wterm-host\s*\{[^}]*overflow-y:\s*auto/s);
      expect(source).toMatch(/\.wterm-host\s*\{[^}]*overflow-x:\s*hidden/s);
    });
    it("overrides wterm inline locked height so flex sizing owns the host box", () => {
      expect(wtermTerminalViewSource).toMatch(/\.wterm-host\s*\{[^}]*flex:\s*1/s);
      expect(wtermTerminalViewSource).toMatch(
        /\.wterm-host\s*\{[^}]*height:\s*auto\s*!important/s,
      );
      expect(wtermTerminalViewSource).toMatch(/\.wterm-host\s*\{[^}]*overflow-y:\s*auto/s);
      expect(wtermTerminalViewSource).toMatch(/\.wterm-host\s*\{[^}]*overflow-x:\s*hidden/s);
    });
    it("neutralizes renderer-written per-row backgrounds and shadows without targeting ANSI cell spans", () => {
      expect(wtermTerminalViewSource).toMatch(
        /:global\(\.wterm-host\.wterm \.term-row\)\s*\{[^}]*background:\s*var\(--term-bg,\s*#1e1e1e\)\s*!important/s,
      );
      expect(wtermTerminalViewSource).toMatch(
        /:global\(\.wterm-host\.wterm \.term-row\)\s*\{[^}]*box-shadow:\s*none\s*!important/s,
      );
      expect(wtermTerminalViewSource).not.toMatch(/\.term-row\s*>\s*span/);
    });
    it("freezes the local grid while the keyboard is open so it stays in lockstep with the PTY", async () => {
      await mountWterm();
      await waitFor(() => expect(termOnResize).toBeTypeOf("function"));

      document.documentElement.classList.add("keyboard-open");
      sendResize.mockClear();
      termOnResize!(40, 20);
      expect(sendResize).not.toHaveBeenCalled();

      document.documentElement.classList.remove("keyboard-open");
      termOnResize!(40, 20);
      expect(sendResize).toHaveBeenLastCalledWith(40, 20);
    });
    it("flushes exactly one server resize once the keyboard closes", async () => {
      await mountWterm();
      await waitFor(() => expect(termOnResize).toBeTypeOf("function"));

      document.documentElement.classList.add("keyboard-open");
      sendResize.mockClear();
      termOnResize!(50, 18);
      expect(sendResize).not.toHaveBeenCalled();

      sendResize.mockClear();
      document.documentElement.classList.remove("keyboard-open");

      await waitFor(() => {
        expect(sendResize).toHaveBeenCalledTimes(1);
      });
      expect(sendResize).toHaveBeenCalledWith(50, 18);
    });
    it("drops safe-area bottom pad on bottom controls while keyboard is open", () => {
      expect(wtermTerminalViewSource).toMatch(
        /\.terminal-keys\s*\{[^}]*padding-bottom:\s*max\(2px,\s*env\(safe-area-inset-bottom\)\)/,
      );
      expect(wtermTerminalViewSource).toMatch(
        /:global\(html\.keyboard-open\)\s+\.terminal-keys\s*\{[^}]*padding-bottom:\s*6px/,
      );
    });
    it("snaps to the newest output when the keyboard opens while scrolled up", async () => {
      const { getByTestId, queryByRole } = await mountWterm();
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      Object.defineProperty(host, "scrollHeight", { configurable: true, value: 340 });
      Object.defineProperty(host, "clientHeight", { configurable: true, value: 170 });

      host.scrollTop = 0;
      await fireEvent.scroll(host);
      onOutput!("background update");
      await tick();
      expect(queryByRole("button", { name: "New output ↓" })).not.toBeNull();

      document.documentElement.classList.add("keyboard-open");
      await tick();

      expect(host.scrollTop).toBe(340);
      expect(queryByRole("button", { name: "New output ↓" })).toBeNull();
    });
  });

  describe("parity gaps: fullscreen and expand chrome", () => {
    it("toggles an expanded terminal mode from the corner fullscreen button", async () => {
      const { getByRole, getByTestId, unmount } = await mountWterm();
      const toggle = getByRole("button", { name: "Expand terminal" });
      const panel = getByTestId("task-terminal-panel");

      expect(document.documentElement.classList.contains("terminal-expanded")).toBe(false);
      expect(toggle.getAttribute("aria-pressed")).toBe("false");
      expect(panel.classList.contains("is-expanded")).toBe(false);

      await fireEvent.click(toggle);
      await tick();
      expect(document.documentElement.classList.contains("terminal-expanded")).toBe(true);
      expect(toggle.getAttribute("aria-pressed")).toBe("true");
      expect(panel.classList.contains("is-expanded")).toBe(true);

      await fireEvent.click(toggle);
      await tick();
      expect(document.documentElement.classList.contains("terminal-expanded")).toBe(false);
      expect(toggle.getAttribute("aria-pressed")).toBe("false");
      expect(panel.classList.contains("is-expanded")).toBe(false);

      await fireEvent.click(toggle);
      await tick();
      unmount();
      expect(document.documentElement.classList.contains("terminal-expanded")).toBe(false);
    });
    it("focuses the terminal on the first fullscreen tap so iOS opens the keyboard", async () => {
      const { getByRole } = await mountWterm();
      const toggle = getByRole("button", { name: "Expand terminal" });
      termFocus.mockClear();

      await fireEvent.click(toggle);
      await tick();

      expect(toggle.getAttribute("aria-pressed")).toBe("true");
      expect(termFocus).toHaveBeenCalled();
    });
    it("blurs the terminal when exiting fullscreen so iOS closes the keyboard", async () => {
      const { getByRole, getByTestId } = await mountWterm();
      const toggle = getByRole("button", { name: "Expand terminal" });
      const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
      const input = document.createElement("input");
      host.appendChild(input);

      await fireEvent.click(toggle);
      input.focus();
      expect(document.activeElement).toBe(input);

      await fireEvent.click(toggle);
      await tick();

      expect(document.activeElement).not.toBe(input);
      input.remove();
    });
    it("resizes the grid on expand even while the keyboard is open", async () => {
      const { getByRole } = await mountWterm();
      await waitFor(() => expect(termOnResize).toBeTypeOf("function"));

      document.documentElement.classList.add("keyboard-open");
      sendResize.mockClear();

      await fireEvent.click(getByRole("button", { name: "Expand terminal" }));
      await tick();

      termOnResize!(60, 22);
      expect(sendResize).toHaveBeenCalledWith(60, 22);
    });
  });

  describe("parity gaps: clipboard depth", () => {
    it("surfaces a clipboard read failure with a paste fallback sheet instead of silently doing nothing", async () => {
      Object.defineProperty(navigator, "clipboard", {
        value: { readText: vi.fn().mockRejectedValue(new Error("denied")) },
        configurable: true,
      });
      const { getByRole, getByTestId } = await mountWterm();

      await fireEvent.click(getByRole("button", { name: "Paste" }));

      await waitFor(() => expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument());
      expect(sendInput).not.toHaveBeenCalledWith(expect.stringContaining("denied"));
    });

    it("sends paste fallback textarea value through the paste encoding path and closes the tray", async () => {
      Object.defineProperty(navigator, "clipboard", {
        value: { readText: vi.fn().mockRejectedValue(new Error("denied")) },
        configurable: true,
      });
      const { getByRole, getByTestId, queryByTestId } = await mountWterm();
      await fireEvent.click(getByRole("button", { name: "Paste" }));
      await waitFor(() => expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument());

      const textarea = getByTestId("terminal-paste-fallback").querySelector("textarea")!;
      await fireEvent.input(textarea, { target: { value: "hello from tray" } });
      termFocus.mockClear();
      await fireEvent.click(getByRole("button", { name: "Send" }));

      await waitFor(() => expect(sendInput).toHaveBeenCalledWith("hello from tray"));
      expect(termFocus).toHaveBeenCalled();
      expect(queryByTestId("terminal-paste-fallback")).toBeNull();
    });

    it("does not paste when Send is tapped with an empty fallback value", async () => {
      Object.defineProperty(navigator, "clipboard", {
        value: { readText: vi.fn().mockRejectedValue(new Error("denied")) },
        configurable: true,
      });
      const { getByRole, getByTestId, queryByTestId } = await mountWterm();
      await fireEvent.click(getByRole("button", { name: "Paste" }));
      await waitFor(() => expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument());

      sendInput.mockClear();
      await fireEvent.click(getByRole("button", { name: "Send" }));

      expect(sendInput).not.toHaveBeenCalled();
      expect(queryByTestId("terminal-paste-fallback")).toBeNull();
    });

    it.todo("opens a readonly copy fallback when clipboard write fails");
  });

  describe("parity gaps: reconnect depth", () => {
    it("resets the buffer and resends PTY size on reconnect without Ajax force-fit", async () => {
      await mountWterm();
      // First open: nothing stale — must NOT reset.
      connectionEvents!.onOpen();
      expect(termWrite).not.toHaveBeenCalledWith("\x1bc");

      termWrite.mockClear();
      sendResize.mockClear();
      termResize.mockClear();

      // Second open = reconnect: clear stale grid, then re-announce fixed PTY size.
      connectionEvents!.onOpen();

      expect(termWrite).toHaveBeenCalledWith("\x1bc");
      expect(sendResize).toHaveBeenCalledWith(80, 24);
      expect(termResize).not.toHaveBeenCalled();
    });
  });

  describe("parity gaps: iOS device-only (bake-off, not unit-testable)", () => {
    it.todo("holds backspace key-repeat despite wterm's preventDefault keydown path (bake-off item 2)");
  });
});
