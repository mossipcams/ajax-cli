import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import WtermTerminalView from "./WtermTerminalView.svelte";

const termWrite = vi.fn();
const termFocus = vi.fn();
const termDestroy = vi.fn();
const termResize = vi.fn();
let termOnData: ((data: string) => void) | undefined;
let termOnResize: ((cols: number, rows: number) => void) | undefined;

const coreBracketedPaste = vi.hoisted(() => vi.fn(() => false));
const coreCursorKeysApp = vi.hoisted(() => vi.fn(() => false));
const loadWtermGhosttyCore = vi.hoisted(() =>
  vi.fn(() =>
    Promise.resolve({
      runtime: "ghostty-core",
      bracketedPaste: coreBracketedPaste,
      cursorKeysApp: coreCursorKeysApp,
    }),
  ),
);

vi.mock("../terminalWtermGhosttyCore", () => ({
  loadWtermGhosttyCore,
}));

vi.mock("@wterm/dom", () => ({
  WTerm: class MockWTerm {
    cols = 72;
    rows = 24;
    constructor(
      _el: HTMLElement,
      options?: {
        onData?: (data: string) => void;
        onResize?: (cols: number, rows: number) => void;
      },
    ) {
      termOnData = options?.onData;
      termOnResize = options?.onResize;
    }
    init = vi.fn(() => Promise.resolve(this));
    resize = termResize;
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
  vi.clearAllMocks();
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
});

afterEach(() => vi.restoreAllMocks());

describe("WtermTerminalView", () => {
  it('exposes data-terminal-engine="wterm"', async () => {
    const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => {
      expect(getByTestId("task-terminal-panel").getAttribute("data-terminal-engine")).toBe("wterm");
    });
  });

  it("loads the wterm Ghostty core via the validated loader", async () => {
    render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(loadWtermGhosttyCore).toHaveBeenCalledTimes(1));
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

  it("force-fits the terminal after init when the host has non-zero dimensions", async () => {
    let resolveLoad: (value: { runtime: string }) => void = () => {};
    const loadPromise = new Promise<{ runtime: string }>((resolve) => {
      resolveLoad = resolve;
    });
    loadWtermGhosttyCore.mockImplementationOnce(() => loadPromise);

    const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
    const host = getByTestId("task-terminal-panel").querySelector(".wterm-host") as HTMLElement;
    Object.defineProperty(host, "clientWidth", { configurable: true, value: 320 });
    Object.defineProperty(host, "clientHeight", { configurable: true, value: 170 });
    resolveLoad({ runtime: "ghostty-core" });

    await waitFor(() => expect(termResize).toHaveBeenCalledWith(40, 10));
  });

  it("unmount calls connection.dispose and term.destroy", async () => {
    const { unmount } = render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(sendResize).toHaveBeenCalled());
    unmount();
    expect(dispose).toHaveBeenCalled();
    expect(termDestroy).toHaveBeenCalled();
  });

  it("init failure invokes onInitFailure and does not leave a live connection", async () => {
    loadWtermGhosttyCore.mockRejectedValueOnce(new Error("wasm missing"));
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
    // WTerm natively re-pins on write; the component never exposes whether the
    // user is scrolled away, so there is nowhere to hang the Ghostty-style
    // "New output" affordance yet.
    it.todo("shows a New output control while the user is scrolled away from bottom");
  });

  describe("parity gaps: geometry and font", () => {
    it.todo("fits the initial grid with wterm-measured cell metrics instead of the hardcoded 8x17 estimate");
    it.todo("uses agent-sized floor of 80 columns on a narrow host");
    it.todo("uses a readable font size on a mobile viewport and a compact one on desktop");
    it.todo("applies a persisted font size on mount and ignores out-of-range values");
    it.todo("grows/shrinks the font on pinch with clamps, persisting the choice");
    it.todo("pans the terminal horizontally on a sideways drag and clamps at the canvas edge");
  });

  describe("parity gaps: keyboard lockstep", () => {
    it.todo("refits immediately but debounces server resize when the visual viewport changes");
    it.todo("freezes the local grid while the keyboard is open so it stays in lockstep with the PTY");
    it.todo("flushes exactly one server resize once the keyboard closes");
    it.todo("drops safe-area bottom pad on bottom controls while keyboard is open");
    it.todo("snaps to the newest output when the keyboard opens while scrolled up");
  });

  describe("parity gaps: fullscreen and expand chrome", () => {
    it.todo("toggles an expanded terminal mode from the corner fullscreen button");
    it.todo("focuses the terminal on the first fullscreen tap so iOS opens the keyboard");
    it.todo("blurs the terminal when exiting fullscreen so iOS closes the keyboard");
    it.todo("resizes the grid on expand even while the keyboard is open");
  });

  describe("parity gaps: clipboard depth", () => {
    it.todo("surfaces a clipboard read failure with a paste fallback sheet instead of silently doing nothing");
    it.todo("opens a readonly copy fallback when clipboard write fails");
  });

  describe("parity gaps: key-bar focus discipline", () => {
    it.todo("does not focus the terminal from a key-bar key, so a closed keyboard stays closed");
    it.todo("refocuses without scrolling when a key-bar key is tapped mid-typing");
  });

  describe("parity gaps: reconnect depth", () => {
    it.todo("refits the grid, resets the buffer, and snaps to bottom on reconnect");
  });

  describe("parity gaps: iOS device-only (bake-off, not unit-testable)", () => {
    it.todo("holds backspace key-repeat despite wterm's preventDefault keydown path (bake-off item 2)");
  });
});
