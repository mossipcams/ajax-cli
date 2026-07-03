/// <reference types="vite/client" />

import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, waitFor, queryByRole } from "@testing-library/svelte";
import { tick } from "svelte";
import TerminalRawView from "./TerminalRawView.svelte";
import terminalRawViewSource from "./TerminalRawView.svelte?raw";

const write = vi.fn();
const scrollToBottom = vi.fn();
const scrollLines = vi.fn();
const dispose = vi.fn();
let onDataHandler: ((data: string) => void) | undefined;
const fit = vi.fn();
const fitDispose = vi.fn();

const focus = vi.fn();
const blur = vi.fn();
const paste = vi.fn();
const resize = vi.fn();
let lastTextarea: HTMLTextAreaElement | undefined;
let terminalOptions: unknown;
let liveOptions: { fontSize?: number } | undefined;
let onScrollHandler: ((viewportY: number) => void) | undefined;
let viewportY = 0;
let proposedDimensions: { cols: number; rows: number } | undefined;
const ghosttyLoad = vi.hoisted(() => vi.fn(() => Promise.resolve({ runtime: "ghostty" })));

vi.mock("ghostty-web", () => ({
  Ghostty: {
    load: ghosttyLoad,
  },
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    textarea = document.createElement("textarea");
    element = document.createElement("div");
    buffer = { active: { viewportY: 0, baseY: 0 } };
    loadAddon = vi.fn();
    open = vi.fn((container: HTMLElement) => {
      container.appendChild(document.createElement("canvas"));
    });
    write = write;
    scrollToBottom = scrollToBottom;
    scrollLines = scrollLines;
    dispose = dispose;
    focus = focus;
    blur = blur;
    paste = paste;
    resize = (cols: number, rows: number) => {
      this.cols = cols;
      this.rows = rows;
      resize(cols, rows);
    };
    onData = vi.fn((handler: (data: string) => void) => {
      onDataHandler = handler;
      return { dispose: vi.fn() };
    });
    onScroll = vi.fn((handler: (viewportY: number) => void) => {
      onScrollHandler = handler;
      return { dispose: vi.fn() };
    });
    getViewportY = () => viewportY;
    options: { fontSize?: number };
    constructor(options: unknown) {
      terminalOptions = options;
      this.options = { fontSize: (options as { fontSize?: number }).fontSize };
      liveOptions = this.options;
      lastTextarea = this.textarea;
    }
  },
  FitAddon: class MockFitAddon {
    fit = fit;
    dispose = fitDispose;
    proposeDimensions = () => proposedDimensions;
  },
  type: {},
}));

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  static OPEN = 1;
  static CLOSED = 3;
  readyState = MockWebSocket.OPEN;
  url: string;
  send = vi.fn();
  close = vi.fn();
  listeners: Record<string, Array<(event?: Event) => void>> = {};

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  addEventListener(type: string, handler: (event?: Event) => void) {
    this.listeners[type] = this.listeners[type] ?? [];
    this.listeners[type].push(handler);
  }

  emit(type: string, event?: Event) {
    for (const handler of this.listeners[type] ?? []) {
      handler(event);
    }
  }
}

const vvListeners: Record<string, Array<() => void>> = {};

function dispatchVisualViewport(type: string) {
  for (const handler of vvListeners[type] ?? []) handler();
}

beforeEach(() => {
  MockWebSocket.instances = [];
  onDataHandler = undefined;
  lastTextarea = undefined;
  terminalOptions = undefined;
  onScrollHandler = undefined;
  viewportY = 0;
  proposedDimensions = undefined;
  liveOptions = undefined;
  write.mockClear();
  scrollToBottom.mockClear();
  dispose.mockClear();
  fit.mockClear();
  fitDispose.mockClear();
  focus.mockClear();
  blur.mockClear();
  paste.mockClear();
  resize.mockClear();
  scrollLines.mockClear();
  ghosttyLoad.mockClear();
  window.localStorage.clear();
  delete (navigator as { clipboard?: unknown }).clipboard;
  for (const key of Object.keys(vvListeners)) delete vvListeners[key];
  vi.stubGlobal("WebSocket", MockWebSocket as unknown as typeof WebSocket);
  vi.spyOn(window, "scrollTo").mockImplementation(() => {});
  vi.stubGlobal(
    "ResizeObserver",
    class MockResizeObserver {
      observe = vi.fn();
      disconnect = vi.fn();
    },
  );
  vi.stubGlobal("visualViewport", {
    addEventListener: (type: string, handler: () => void) => {
      (vvListeners[type] ??= []).push(handler);
    },
    removeEventListener: vi.fn(),
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function stubMatchMedia(matcher: (query: string) => boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => ({
      matches: matcher(query),
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  );
}

/** Mount the terminal for the standard test handle; expose the socket and the
 * touch-scroll host alongside the render utils. */
async function mountTerminal() {
  const utils = render(TerminalRawView, { props: { handle: "web/fix-login" } });
  await waitFor(() => expect(onDataHandler).toBeDefined());
  const socket = MockWebSocket.instances[0];
  const host = utils.container.querySelector(".task-terminal-viewport") as HTMLElement;
  return { ...utils, socket, host };
}

/** mountTerminal plus a successful socket open. */
async function mountOpenTerminal() {
  const mounted = await mountTerminal();
  mounted.socket?.emit("open");
  return mounted;
}

/** Let the open-triggered post-layout refits (double rAF) settle. */
const settleFrames = () =>
  new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));

/** Simulate the user scrolling away from the bottom of the scrollback. */
function scrollAwayFromBottom() {
  viewportY = 3;
  onScrollHandler?.(3);
}

const resizeFramesOf = (socket: MockWebSocket) =>
  socket.send.mock.calls
    .map((call) => JSON.parse(call[0] as string))
    .filter((frame) => frame.type === "resize");

/** Net scrollback movement: the behavior contract is how far the view moved,
 * not how many scrollLines calls delivered it. */
const linesScrolled = () =>
  scrollLines.mock.calls.reduce((sum, call) => sum + (call[0] as number), 0);

// Raw-first contract: the task terminal is the raw ghostty/tmux bridge on every
// viewport. No Live/snapshot/composer mode may come back as the default.
describe("raw-first task terminal contract", () => {
  it("uses ghostty-web as the terminal renderer without xterm runtime imports", () => {
    expect(terminalRawViewSource).toContain("ghostty-web");
    expect(terminalRawViewSource).not.toContain("@xterm/xterm");
    expect(terminalRawViewSource).not.toContain("@xterm/addon-fit");
    expect(terminalRawViewSource).not.toContain("xterm-zerolag-input");
    expect(terminalRawViewSource).not.toContain(".xterm");
  });

  it("defaults to the raw terminal socket on a mobile viewport", async () => {
    stubMatchMedia((query) => query.includes("max-width: 767px"));

    const { container } = render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(MockWebSocket.instances).toHaveLength(1);
      expect(MockWebSocket.instances[0]?.url).toContain("/api/tasks/web%2Ffix-login/terminal");
    });
    expect(queryByRole(container, "tablist", { name: "Terminal mode" })).not.toBeInTheDocument();
  });

  it("defaults to the raw terminal socket on desktop", async () => {
    stubMatchMedia(() => false);

    const { container } = render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(MockWebSocket.instances).toHaveLength(1);
    });
    expect(queryByRole(container, "tablist", { name: "Terminal mode" })).not.toBeInTheDocument();
  });

  it("does not render snapshot viewer or mode tabs on any viewport", async () => {
    stubMatchMedia((query) => query.includes("max-width: 767px"));

    const { container } = render(TerminalRawView, { props: { handle: "web/fix-login" } });
    await waitFor(() => expect(MockWebSocket.instances).toHaveLength(1));

    expect(queryByRole(container, "tab", { name: "Live" })).not.toBeInTheDocument();
    expect(queryByRole(container, "tab", { name: "Raw terminal" })).not.toBeInTheDocument();
    expect(container.querySelector(".terminal-snapshot-lines")).not.toBeInTheDocument();
    expect(localStorage.getItem("ajax.terminal.mode")).toBeNull();
  });
});

describe("TerminalRawView", () => {
  it("opens the task terminal socket on mount", async () => {
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(MockWebSocket.instances).toHaveLength(1);
      expect(MockWebSocket.instances[0]?.url).toContain("/api/tasks/web%2Ffix-login/terminal");
    });
  });

  it("writes incoming output frames to the terminal", async () => {
    const { socket } = await mountTerminal();
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("hello") }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("hello");
    });
  });

  it("decodes UTF-8 output frames before writing to the terminal", async () => {
    const { socket } = await mountTerminal();
    const bytes = new TextEncoder().encode("λ ready");
    const encoded = btoa(String.fromCharCode(...bytes));

    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: encoded }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("λ ready");
    });
  });

  it("writes a non-JSON frame through as raw text", async () => {
    render(TerminalRawView, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];

    socket?.emit("message", { data: "plain text frame" } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("plain text frame");
    });
  });

  it("decodes Blob websocket messages before writing to the terminal", async () => {
    const { socket } = await mountTerminal();
    const payload = JSON.stringify({ type: "output", data: btoa("blob ready") });

    socket?.emit("message", {
      data: new Blob([payload], { type: "application/json" }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("blob ready");
    });
  });

  it("sends_clear_command_text_and_enter_over_the_raw_socket", async () => {
    const { socket } = await mountOpenTerminal();

    for (const char of "clear") {
      onDataHandler?.(char);
    }
    onDataHandler?.("\r");

    await waitFor(() => {
      for (const char of "clear") {
        expect(socket?.send).toHaveBeenCalledWith(
          JSON.stringify({ type: "input", data: char }),
        );
      }
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\r" }),
      );
    });
  });

  it("sends terminal input as JSON frames", async () => {
    const { socket } = await mountOpenTerminal();

    onDataHandler?.("a");

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "a" }),
      );
    });
  });

  it("forwards successive printable input immediately", async () => {
    const { socket } = await mountOpenTerminal();

    onDataHandler?.("h");
    onDataHandler?.("i");

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(JSON.stringify({ type: "input", data: "h" }));
      expect(socket?.send).toHaveBeenCalledWith(JSON.stringify({ type: "input", data: "i" }));
    });
  });

  it("sends Enter as a raw terminal frame", async () => {
    const { socket } = await mountOpenTerminal();

    onDataHandler?.("\r");

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\r" }),
      );
    });
  });

  it("always forwards backspace to the PTY", async () => {
    const { socket } = await mountOpenTerminal();

    onDataHandler?.("\x7f");

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\x7f" }),
      );
    });
  });

  it("loads ghostty-web with the served wasm asset", async () => {
    await mountTerminal();

    expect(ghosttyLoad).toHaveBeenCalledWith("/ghostty-vt.wasm");
    expect((terminalOptions as { ghostty?: unknown }).ghostty).toEqual({ runtime: "ghostty" });
  });

  it("exposes stable layout hooks for the task terminal viewport", () => {
    const { container, getByLabelText } = render(TerminalRawView, {
      props: { handle: "web/fix-login" },
    });

    expect(getByLabelText("Task terminal")).toBeInTheDocument();
    expect(container.querySelector("[data-testid='task-terminal-panel']")).toHaveAttribute(
      "data-terminal-engine",
      "ghostty",
    );
    expect(container.querySelector(".task-terminal-viewport")).toBeInTheDocument();
    expect(container.querySelector("[data-testid='terminal-bottom-controls']")).toBeInTheDocument();
    // The composer is gone for good: the raw terminal is the only input surface.
    expect(container.querySelector("[data-testid='terminal-composer']")).toBeNull();
  });

  it("pins the fullscreen toggle to the terminal's top-right corner", () => {
    const { container } = render(TerminalRawView, { props: { handle: "web/fix-login" } });

    const toggle = container.querySelector(".terminal-expand-corner");
    expect(toggle).toBeInTheDocument();
    expect(toggle?.getAttribute("aria-label")).toBe("Expand terminal");
    // Overlay, not a key-bar item: absolutely positioned at the panel's top right.
    expect(terminalRawViewSource).toMatch(/\.terminal-expand-corner\s*\{[^}]*position:\s*absolute/);
    expect(terminalRawViewSource).toMatch(/\.terminal-expand-corner\s*\{[^}]*top:/);
    expect(terminalRawViewSource).toMatch(/\.terminal-expand-corner\s*\{[^}]*right:/);
    expect(terminalRawViewSource).toMatch(/\.terminal-panel\s*\{[^}]*position:\s*relative/);
  });

  it("keeps the terminal viewport as the internal flex scrollback area", () => {
    expect(terminalRawViewSource).toMatch(/\.terminal-panel\s*\{[^}]*display:\s*flex/);
    expect(terminalRawViewSource).toMatch(/\.terminal-panel\s*\{[^}]*overflow:\s*hidden/);
    expect(terminalRawViewSource).toMatch(/\.terminal-host\s*\{[^}]*flex:\s*1 1 auto/);
    expect(terminalRawViewSource).toMatch(/\.terminal-host\s*\{[^}]*min-height:\s*0/);
    expect(terminalRawViewSource).toMatch(/\.terminal-host\s*\{[^}]*overflow:\s*hidden/);
    expect(terminalRawViewSource).toMatch(/\.terminal-bottom-controls\s*\{[^}]*flex:\s*none/);
    expect(terminalRawViewSource).toMatch(/\.terminal-bottom-controls\s*\{[^}]*padding-bottom:\s*max\([^;]*env\(safe-area-inset-bottom\)/);
  });

  it("closes the socket and disposes ghostty on destroy", async () => {
    const { unmount, socket } = await mountTerminal();

    unmount();

    await waitFor(() => {
      expect(socket?.close).toHaveBeenCalled();
      expect(dispose).toHaveBeenCalled();
    });
  });

  it("keeps the newest output in view after writes and viewport resizes", async () => {
    const { socket } = await mountOpenTerminal();

    scrollToBottom.mockClear();
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("hi") }),
    } as MessageEvent);
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());

    // A keyboard-driven viewport shrink refits the terminal; the cursor row must
    // not be stranded above the fold afterwards.
    scrollToBottom.mockClear();
    dispatchVisualViewport("resize");
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
  });

  it("does not yank the view back down while the user has scrolled up", async () => {
    const { socket } = await mountOpenTerminal();

    // Let the open-triggered post-layout refits (which unconditionally
    // scroll to bottom) settle before simulating the user scrolling up.
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    await settleFrames();

    // Simulate the user scrolling away from the bottom of the scrollback.
    scrollAwayFromBottom();

    scrollToBottom.mockClear();
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("status bar redraw") }),
    } as MessageEvent);

    await waitFor(() => expect(write).toHaveBeenCalledWith("status bar redraw"));
    expect(scrollToBottom).not.toHaveBeenCalled();

    // Once the user scrolls back to the bottom, auto-follow resumes.
    viewportY = 0;
    onScrollHandler?.(10);
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("more output") }),
    } as MessageEvent);

    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
  });

  it("shows a New output control while the user is scrolled away from bottom", async () => {
    const { getByRole, queryByRole } = render(TerminalRawView, {
      props: { handle: "web/fix-login" },
    });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    await settleFrames();

    scrollAwayFromBottom();

    scrollToBottom.mockClear();
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("background update") }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("background update");
      expect(getByRole("button", { name: "New output ↓" })).toBeInTheDocument();
    });
    expect(scrollToBottom).not.toHaveBeenCalled();

    focus.mockClear();
    getByRole("button", { name: "New output ↓" }).click();

    expect(scrollToBottom).toHaveBeenCalled();
    // Jumping to the newest output is a *reading* action; focusing here would
    // pop the iOS keyboard and shrink the very output the user asked to see
    // (the same contract as the expand toggle).
    expect(focus).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(queryByRole("button", { name: "New output ↓" })).not.toBeInTheDocument();
    });
  });

  it("refits immediately but debounces server resize when the visual viewport changes", async () => {
    vi.useFakeTimers();
    const { socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(50);
    fit.mockClear();
    socket!.send.mockClear();

    dispatchVisualViewport("resize");

    vi.advanceTimersByTime(20);
    expect(fit).toHaveBeenCalled();
    expect(socket?.send).not.toHaveBeenCalled();

    vi.advanceTimersByTime(279);
    expect(socket?.send).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(socket?.send).toHaveBeenCalledWith(
      JSON.stringify({ type: "resize", cols: 80, rows: 24 }),
    );
    vi.useRealTimers();
  });

  // Keyboard state is the shared `keyboard-open` class viewport.ts maintains
  // (the same signal the CSS takeover uses), so the tests drive that class.
  const setKeyboardOpen = (open: boolean) =>
    document.documentElement.classList.toggle("keyboard-open", open);

  it("freezes the local grid while the keyboard is open so it stays in lockstep with the PTY", async () => {
    // The server resize is withheld while the keyboard is open, so the local
    // grid must not shrink either: a grid smaller than the PTY makes tmux
    // cursor-address rows that no longer exist locally, and the renderer clamps
    // those writes to its bottom row — the TUI input box drifts up and
    // overwrites the line below it.
    vi.useFakeTimers();
    const { socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400); // let the open-path refits settle
    fit.mockClear();
    resize.mockClear();
    socket!.send.mockClear();

    const resizeFrames = () => resizeFramesOf(socket!);

    // Keyboard opens: viewport.ts flags it and the viewport resizes.
    setKeyboardOpen(true);
    dispatchVisualViewport("resize");
    vi.advanceTimersByTime(500);
    setKeyboardOpen(false);

    expect(fit).not.toHaveBeenCalled(); // grid untouched while open
    expect(resize).not.toHaveBeenCalled();
    expect(resizeFrames()).toHaveLength(0); // no server resize while open
    vi.useRealTimers();
  });

  it("anchors the visible crop to the canvas bottom while the keyboard is open", async () => {
    vi.useFakeTimers();
    const { socket, host } = await mountTerminal();
    socket?.emit("open");
    vi.advanceTimersByTime(400);

    // The frozen grid is taller than the keyboard-shrunken host; the crop
    // must show the bottom of the canvas (cursor/input row), not the top.
    Object.defineProperty(host, "scrollHeight", { value: 800, configurable: true });
    Object.defineProperty(host, "clientHeight", { value: 300, configurable: true });

    setKeyboardOpen(true);
    dispatchVisualViewport("resize");
    vi.advanceTimersByTime(500);
    setKeyboardOpen(false);

    expect(host.scrollTop).toBe(500);
    vi.useRealTimers();
  });

  it("flushes exactly one server resize once the keyboard closes", async () => {
    vi.useFakeTimers();
    const { socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400);
    socket!.send.mockClear();

    const resizeFrames = () => resizeFramesOf(socket!);

    // Open the keyboard (several animation frames), then close it.
    setKeyboardOpen(true);
    dispatchVisualViewport("resize");
    vi.advanceTimersByTime(100);
    dispatchVisualViewport("resize");
    vi.advanceTimersByTime(100);
    setKeyboardOpen(false);
    dispatchVisualViewport("resize");
    vi.advanceTimersByTime(300);

    expect(resizeFrames()).toHaveLength(1);
    vi.useRealTimers();
  });

  it("does not scroll to bottom on viewport resize while the user is scrolled up", async () => {
    await mountOpenTerminal();

    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    await settleFrames();

    scrollAwayFromBottom();
    scrollToBottom.mockClear();

    dispatchVisualViewport("resize");
    await waitFor(() => expect(fit).toHaveBeenCalled());

    expect(scrollToBottom).not.toHaveBeenCalled();
  });

  it("runs a second post-layout resize after the socket opens", async () => {
    const { socket } = await mountTerminal();
    await settleFrames();
    fit.mockClear();
    socket!.send.mockClear();

    socket?.emit("open");
    await settleFrames();
    await settleFrames();

    expect(fit.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(socket?.send).toHaveBeenCalledTimes(2);
    expect(socket?.send).toHaveBeenNthCalledWith(
      1,
      JSON.stringify({ type: "resize", cols: 80, rows: 24 }),
    );
    expect(socket?.send).toHaveBeenNthCalledWith(
      2,
      JSON.stringify({ type: "resize", cols: 80, rows: 24 }),
    );
  });

  it("floors the PTY at 80 columns when the viewport proposes fewer", async () => {
    proposedDimensions = { cols: 55, rows: 30 };
    const { socket } = await mountTerminal();

    socket?.emit("open");

    await waitFor(() => {
      expect(resize).toHaveBeenCalledWith(80, 30);
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "resize", cols: 80, rows: 30 }),
      );
    });
  });

  it("keeps a wide fit proposal above the column floor untouched", async () => {
    proposedDimensions = { cols: 120, rows: 40 };
    const { socket } = await mountTerminal();

    socket?.emit("open");

    await waitFor(() => {
      expect(resize).toHaveBeenCalledWith(120, 40);
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "resize", cols: 120, rows: 40 }),
      );
    });
  });

  it("falls back to a plain fit when no dimensions can be proposed", async () => {
    proposedDimensions = undefined;
    const { socket } = await mountTerminal();
    fit.mockClear();

    socket?.emit("open");

    await waitFor(() => {
      expect(fit).toHaveBeenCalled();
      expect(resize).not.toHaveBeenCalled();
    });
  });

  it("enters reconnecting and opens a new socket after the socket closes", async () => {
    vi.useFakeTimers();
    const { getByTestId } = render(TerminalRawView, { props: { handle: "web/fix-login" } });
    const first = MockWebSocket.instances[0];
    first?.emit("open");

    first!.readyState = MockWebSocket.CLOSED;
    first?.emit("close");
    await tick();

    expect(getByTestId("terminal-status").textContent).toContain("Reconnecting");
    expect(MockWebSocket.instances).toHaveLength(1);

    // First backoff is 1s.
    vi.advanceTimersByTime(1000);
    expect(MockWebSocket.instances).toHaveLength(2);
    vi.useRealTimers();
  });

  it("reconnects immediately when the tab returns to the foreground", async () => {
    render(TerminalRawView, { props: { handle: "web/fix-login" } });
    const first = MockWebSocket.instances[0];
    first?.emit("open");
    first!.readyState = MockWebSocket.CLOSED;
    first?.emit("close"); // now in reconnecting, waiting out backoff

    Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    document.dispatchEvent(new Event("visibilitychange"));

    // Foreground reconnect fires without waiting for the backoff timer.
    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2);
  });

  it("backs off with a growing delay that resets after a successful open", async () => {
    vi.useFakeTimers();
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    let sock = MockWebSocket.instances[0];
    sock.emit("open");
    sock.readyState = MockWebSocket.CLOSED;
    sock.emit("close"); // schedule at 1s
    vi.advanceTimersByTime(999);
    expect(MockWebSocket.instances).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(MockWebSocket.instances).toHaveLength(2);

    // Second consecutive failure (no successful open) backs off to 2s.
    sock = MockWebSocket.instances[1];
    sock.readyState = MockWebSocket.CLOSED;
    sock.emit("close");
    vi.advanceTimersByTime(1999);
    expect(MockWebSocket.instances).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(MockWebSocket.instances).toHaveLength(3);

    // A successful open resets the backoff: the next failure waits 1s again.
    sock = MockWebSocket.instances[2];
    sock.emit("open");
    sock.readyState = MockWebSocket.CLOSED;
    sock.emit("close");
    vi.advanceTimersByTime(1000);
    expect(MockWebSocket.instances).toHaveLength(4);
    vi.useRealTimers();
  });

  it("refits and focuses ghostty on reconnect", async () => {
    const { socket: first } = await mountTerminal();
    await settleFrames();
    vi.useFakeTimers();
    first?.emit("open");
    await vi.advanceTimersByTimeAsync(1000);

    fit.mockClear();
    focus.mockClear();
    first!.readyState = MockWebSocket.CLOSED;
    first?.emit("close");
    await vi.advanceTimersByTimeAsync(1000);

    const second = MockWebSocket.instances[1];
    second?.emit("open");
    await vi.advanceTimersByTimeAsync(1000);

    expect(fit).toHaveBeenCalled();
    expect(focus).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("refits and focuses ghostty on the first connect", async () => {
    const { socket: first } = await mountTerminal();
    await settleFrames();

    fit.mockClear();
    focus.mockClear();
    first?.emit("open");
    await settleFrames();

    expect(fit).toHaveBeenCalled();
    expect(focus).toHaveBeenCalled();
  });

  it("offers a manual reconnect button that opens a new socket", async () => {
    const { findByRole } = render(TerminalRawView, { props: { handle: "web/fix-login" } });
    const first = MockWebSocket.instances[0];
    first?.emit("open");
    first!.readyState = MockWebSocket.CLOSED;
    first?.emit("close");

    const button = await findByRole("button", { name: "Reconnect" });
    button.click();

    expect(MockWebSocket.instances.length).toBeGreaterThanOrEqual(2);
  });

  it("offers a Hide keyboard key that blurs the terminal", async () => {
    // iPhone keyboards have no dismiss key and the keyboard-open chrome
    // collapse hides the Back button, so without this key the operator is
    // trapped typing with a half-height terminal.
    const { getByRole } = await mountOpenTerminal();

    getByRole("button", { name: "Hide keyboard" }).click();

    await waitFor(() => {
      expect(blur).toHaveBeenCalled();
    });
  });

  it("snaps to the newest output when the keyboard opens while scrolled up", async () => {
    // Opening the keyboard means the user is about to type; the view must jump
    // to the cursor/input row instead of staying parked in scrollback.
    await mountOpenTerminal();
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    await settleFrames();

    scrollAwayFromBottom();
    scrollToBottom.mockClear();

    setKeyboardOpen(true);
    dispatchVisualViewport("resize");
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    setKeyboardOpen(false);
  });

  it("does not focus the terminal from a key-bar key, so a closed keyboard stays closed", async () => {
    // focusTerm() here popped the iOS keyboard: tapping an arrow with the
    // keyboard down shrank the viewport — the terminal appeared to jump.
    const { getByRole } = await mountOpenTerminal();
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    const focusSpy = vi.spyOn(lastTextarea!, "focus");
    focus.mockClear();

    getByRole("button", { name: "←" }).click();

    expect(focus).not.toHaveBeenCalled();
    expect(focusSpy).not.toHaveBeenCalled();
  });

  it("refocuses without scrolling when a key-bar key is tapped mid-typing", async () => {
    // While the terminal owns focus, a key-bar tap must keep the keyboard up —
    // but via preventScroll, so Safari never scroll-chases the hidden textarea.
    const { getByRole } = await mountOpenTerminal();
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    const input = lastTextarea!;
    document.body.appendChild(input);
    input.focus();
    const focusSpy = vi.spyOn(input, "focus");

    getByRole("button", { name: "→" }).click();

    expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
    input.remove();
  });

  it("pastes clipboard text through the terminal paste path", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: vi.fn().mockResolvedValue("git push origin main") },
      configurable: true,
    });
    const { getByRole } = await mountOpenTerminal();

    getByRole("button", { name: "Paste" }).click();

    await waitFor(() => {
      // term.paste() honors bracketed-paste mode and flows through the
      // existing onData → socket path, so the PTY receives it like any input.
      expect(paste).toHaveBeenCalledWith("git push origin main");
      expect(focus).toHaveBeenCalled();
    });
  });

  it("keeps a server error visible after a successful paste", async () => {
    // Clipboard feedback and bridge errors are separate channels: a paste
    // must never clear a server-reported failure it does not own.
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: vi.fn().mockResolvedValue("ls") },
      configurable: true,
    });
    const { getByRole, getByTestId, socket } = await mountOpenTerminal();
    socket?.emit("message", {
      data: JSON.stringify({ type: "error", error: "tmux session missing" }),
    } as MessageEvent);
    await waitFor(() => {
      expect(getByTestId("terminal-status").textContent).toContain("tmux session missing");
    });

    getByRole("button", { name: "Paste" }).click();

    await waitFor(() => expect(paste).toHaveBeenCalledWith("ls"));
    expect(getByTestId("terminal-status").textContent).toContain("tmux session missing");
  });

  it("surfaces a clipboard read failure instead of silently doing nothing", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: vi.fn().mockRejectedValue(new Error("denied")) },
      configurable: true,
    });
    const { getByRole, getByTestId } = render(TerminalRawView, {
      props: { handle: "web/fix-login" },
    });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    getByRole("button", { name: "Paste" }).click();

    await waitFor(() => {
      expect(getByTestId("terminal-status").textContent).toContain("Clipboard");
    });
    expect(paste).not.toHaveBeenCalled();
  });

  it("sends an Escape byte when the Esc key is tapped", async () => {
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: "Esc" }).click();

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\x1b" }),
      );
    });
  });

  it("folds the next letter into a control code after Ctrl is armed", async () => {
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: "Ctrl" }).click();
    onDataHandler?.("c");

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\x03" }),
      );
    });
  });

  it("auto-disarms sticky Ctrl after the timeout so a later key is unmodified", async () => {
    vi.useFakeTimers();
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: /Ctrl/ }).click();
    vi.advanceTimersByTime(4000);

    onDataHandler?.("c");

    // The arm expired, so "c" is sent literally, not folded to \x03.
    expect(socket?.send).toHaveBeenCalledWith(JSON.stringify({ type: "input", data: "c" }));
    expect(socket?.send).not.toHaveBeenCalledWith(JSON.stringify({ type: "input", data: "\x03" }));
    vi.useRealTimers();
  });

  it("sends Enter unchanged when Ctrl is armed", async () => {
    // Ctrl folds letters and cursor keys but leaves Enter as a plain "\r" —
    // which must still take the normal Enter path.
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: /Ctrl/ }).click();
    onDataHandler?.("\r");

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(JSON.stringify({ type: "input", data: "\r" }));
    });
  });

  it("applies an armed Ctrl to a control-bar cursor key, then disarms", async () => {
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: /Ctrl/ }).click();
    getByRole("button", { name: "←" }).click();

    // Ctrl+← becomes the Ctrl-modified CSI cursor sequence.
    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\x1b[1;5D" }),
      );
    });

    // The arm was consumed: a following key is unmodified.
    socket!.send.mockClear();
    onDataHandler?.("x");
    expect(socket?.send).toHaveBeenCalledWith(JSON.stringify({ type: "input", data: "x" }));
  });

  it("snaps the visible viewport on expand: document top, host bottom crop, terminal scroll bottom", async () => {
    vi.useFakeTimers();
    const { getByRole, host } = await mountOpenTerminal();
    vi.advanceTimersByTime(400);

    scrollAwayFromBottom();
    scrollToBottom.mockClear();

    Object.defineProperty(host, "scrollHeight", { value: 800, configurable: true });
    Object.defineProperty(host, "clientHeight", { value: 300, configurable: true });
    document.documentElement.scrollTop = 120;

    getByRole("button", { name: "Expand terminal" }).click();
    await tick();

    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
    expect(document.documentElement.scrollTop).toBe(0);
    if (document.scrollingElement) {
      expect(document.scrollingElement.scrollTop).toBe(0);
    }
    expect(host.scrollTop).toBe(500);
    expect(scrollToBottom).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("toggles an expanded terminal mode from the corner fullscreen button", async () => {
    const { getByRole, unmount } = render(TerminalRawView, { props: { handle: "web/fix-login" } });
    const toggle = getByRole("button", { name: "Expand terminal" });

    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(false);
    expect(toggle.getAttribute("aria-pressed")).toBe("false");

    toggle.click();
    await tick();
    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(true);
    expect(toggle.getAttribute("aria-pressed")).toBe("true");

    toggle.click();
    await tick();
    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(false);
    expect(toggle.getAttribute("aria-pressed")).toBe("false");

    // Leaving the task view while expanded must not leak the takeover class.
    toggle.click();
    await tick();
    unmount();
    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(false);
  });

  it("focuses the terminal on the first fullscreen tap so iOS opens the keyboard", async () => {
    const { getByRole } = await mountOpenTerminal();
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    focus.mockClear();
    blur.mockClear();

    getByRole("button", { name: "Expand terminal" }).click();
    await tick();

    expect(focus).toHaveBeenCalledTimes(1);
    expect(blur).not.toHaveBeenCalled();
  });

  it("blurs the terminal when exiting fullscreen so iOS closes the keyboard", async () => {
    const { getByRole } = await mountOpenTerminal();
    const toggle = getByRole("button", { name: "Expand terminal" });
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    focus.mockClear();
    blur.mockClear();

    toggle.click();
    await tick();
    toggle.click();
    await tick();

    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(false);
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(blur).toHaveBeenCalledTimes(1);
  });

  it("keeps the terminal hotkey row compact", () => {
    expect(terminalRawViewSource).toMatch(/\.terminal-keys\s*\{[^}]*gap:\s*4px/);
    expect(terminalRawViewSource).toMatch(/\.terminal-keys\s*\{[^}]*padding:\s*2px 4px/);
    expect(terminalRawViewSource).toMatch(/\.terminal-key\s*\{[^}]*min-height:\s*28px/);
    expect(terminalRawViewSource).toMatch(/\.terminal-key\s*\{[^}]*font-size:\s*11px/);
  });

  it("refits through the immediate path when expand is toggled", async () => {
    vi.useFakeTimers();
    proposedDimensions = { cols: 55, rows: 30 };
    const { getByRole, socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400); // settle the open-path refits
    socket!.send.mockClear();

    proposedDimensions = { cols: 55, rows: 60 }; // the expanded panel is taller
    getByRole("button", { name: "Expand terminal" }).click();
    // Two animation frames, far below the 300ms debounce window.
    vi.advanceTimersByTime(50);

    expect(resizeFramesOf(socket!)).toContainEqual({ type: "resize", cols: 80, rows: 60 });
    vi.useRealTimers();
  });

  it("disables autocorrect/autocapitalize on the ghostty input", async () => {
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(lastTextarea?.getAttribute("autocapitalize")).toBe("off");
      expect(lastTextarea?.getAttribute("autocorrect")).toBe("off");
      expect(lastTextarea?.getAttribute("spellcheck")).toBe("false");
    });
  });

  it("uses a readable font size on a mobile viewport", async () => {
    // 13px matches desktop: the old 10px default was a column-count lever
    // that the 80-column PTY floor made obsolete.
    stubMatchMedia((query) => query.includes("max-width: 767px"));
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect((terminalOptions as { fontSize: number }).fontSize).toBe(13);
    });
  });

  it("gives a coarse-pointer landscape phone the same readable font as portrait", async () => {
    // A landscape iPhone is wider than the width breakpoint but still a phone;
    // it must get the readable default, never a squintier one.
    stubMatchMedia((query) => query.includes("pointer: coarse"));
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect((terminalOptions as { fontSize: number }).fontSize).toBe(13);
    });
  });

  it("uses a compact font size on a desktop viewport", async () => {
    stubMatchMedia(() => false);
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect((terminalOptions as { fontSize: number }).fontSize).toBeLessThan(14);
    });
  });

  function makeTouch(type: string, clientY: number, clientX = 10): Event {
    const event = new Event(type, { bubbles: true, cancelable: true });
    Object.defineProperty(event, "touches", {
      value: [{ clientX, clientY }],
    });
    return event;
  }

  function sizeHostForPan(host: HTMLElement, scrollWidth = 480, clientWidth = 338) {
    Object.defineProperty(host, "scrollWidth", { value: scrollWidth, configurable: true });
    Object.defineProperty(host, "clientWidth", { value: clientWidth, configurable: true });
  }

  function appendTerminalLayer(host: HTMLElement): HTMLElement {
    const layer = document.createElement("div");
    layer.className = "ghostty-screen";
    host.appendChild(layer);
    return layer;
  }

  it("scrolls local terminal scrollback on touch drag", async () => {
    const { host } = await mountTerminal();

    // Drag the finger up ~60px. With no rendered viewport the cell height falls
    // back to 18px, so that is 3 wheel notches toward the newest output.
    host.dispatchEvent(makeTouch("touchstart", 200));
    const move = makeTouch("touchmove", 140);
    host.dispatchEvent(move);

    expect(linesScrolled()).toBe(3);
    // A moved touch is a scroll, not a tap: default is prevented so iOS does
    // not synthesize the click that would pop the keyboard.
    expect(move.defaultPrevented).toBe(true);
  });

  it("scrolls back into history when the finger drags downward", async () => {
    const { host } = await mountTerminal();

    host.dispatchEvent(makeTouch("touchstart", 100));
    host.dispatchEvent(makeTouch("touchmove", 160));

    expect(linesScrolled()).toBe(-3);
  });

  it("pans the terminal horizontally on a sideways drag", async () => {
    const { host } = await mountTerminal();
    sizeHostForPan(host);

    // Finger moves left 60px with no vertical travel: the canvas pans right.
    host.dispatchEvent(makeTouch("touchstart", 200, 200));
    const move = makeTouch("touchmove", 200, 140);
    host.dispatchEvent(move);

    expect(host.scrollLeft).toBe(60);
    expect(scrollLines).not.toHaveBeenCalled();
    expect(move.defaultPrevented).toBe(true);
  });

  it("clamps the horizontal pan at the canvas edge", async () => {
    const { host } = await mountTerminal();
    sizeHostForPan(host); // 480px canvas in a 338px viewport → max pan 142

    host.dispatchEvent(makeTouch("touchstart", 200, 600));
    host.dispatchEvent(makeTouch("touchmove", 200, 100));

    expect(host.scrollLeft).toBe(142);

    // Panning back past the left edge clamps at zero.
    host.dispatchEvent(makeTouch("touchstart", 200, 100));
    host.dispatchEvent(makeTouch("touchmove", 200, 600));

    expect(host.scrollLeft).toBe(0);
  });

  it("does not pan horizontally during a vertical-only drag", async () => {
    const { host } = await mountTerminal();
    sizeHostForPan(host);

    host.dispatchEvent(makeTouch("touchstart", 200));
    host.dispatchEvent(makeTouch("touchmove", 140));

    expect(scrollLines).toHaveBeenCalled();
    expect(host.scrollLeft).toBe(0);
  });

  function makePinch(type: string, points: Array<{ x: number; y: number }>): Event {
    const event = new Event(type, { bubbles: true, cancelable: true });
    Object.defineProperty(event, "touches", {
      value: points.map((point) => ({ clientX: point.x, clientY: point.y })),
    });
    return event;
  }

  it("applies a persisted font size on mount", async () => {
    window.localStorage.setItem("ajax.terminal.fontSize", "16");
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect((terminalOptions as { fontSize: number }).fontSize).toBe(16);
    });
  });

  it("ignores an out-of-range persisted font size and uses the default", async () => {
    window.localStorage.setItem("ajax.terminal.fontSize", "999");
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect((terminalOptions as { fontSize: number }).fontSize).toBe(13);
    });
  });

  it("grows the font on a pinch spread, clamps it, and persists the choice", async () => {
    const { host } = await mountTerminal();

    // Two fingers land 100px apart (base font 13), spread to 150px:
    // 13 * 1.5 = 19.5 → rounds to 20, which is also the clamp ceiling.
    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 100, y: 100 },
        { x: 200, y: 100 },
      ]),
    );
    const move = makePinch("touchmove", [
      { x: 75, y: 100 },
      { x: 225, y: 100 },
    ]);
    host.dispatchEvent(move);

    expect(liveOptions?.fontSize).toBe(20);
    expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBe("20");
    expect(move.defaultPrevented).toBe(true);
    // A pinch is never a scroll: the buffer must not move.
    expect(scrollLines).not.toHaveBeenCalled();
  });

  it("shrinks the font on a pinch-in and clamps at the readable minimum", async () => {
    const { host } = await mountTerminal();

    // 100px → 20px spread: 13 * 0.2 = 2.6 → clamped up to the 7px floor.
    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 100, y: 100 },
        { x: 200, y: 100 },
      ]),
    );
    host.dispatchEvent(
      makePinch("touchmove", [
        { x: 140, y: 100 },
        { x: 160, y: 100 },
      ]),
    );

    expect(liveOptions?.fontSize).toBe(7);
    expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBe("7");
  });

  it("keeps scrolling with momentum after a fast drag is released", async () => {
    const { host } = await mountTerminal();

    // A fast upward drag (~60px) then release: the drag itself scrolls 3
    // notches (18px fallback cell), and the fling must keep going afterwards.
    host.dispatchEvent(makeTouch("touchstart", 200));
    host.dispatchEvent(makeTouch("touchmove", 140));
    const dragCalls = scrollLines.mock.calls.length;
    expect(dragCalls).toBeGreaterThan(0);
    host.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));

    await waitFor(() => {
      expect(scrollLines.mock.calls.length).toBeGreaterThan(dragCalls);
    });
  });

  it("cancels a running fling the moment a new touch lands", async () => {
    const { host } = await mountTerminal();

    host.dispatchEvent(makeTouch("touchstart", 200));
    host.dispatchEvent(makeTouch("touchmove", 140));
    const dragCalls = scrollLines.mock.calls.length;
    host.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));
    await waitFor(() => {
      expect(scrollLines.mock.calls.length).toBeGreaterThan(dragCalls);
    });

    // Finger down again: the fling stops dead so the user regains control.
    host.dispatchEvent(makeTouch("touchstart", 200));
    const atCancel = scrollLines.mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 120));
    expect(scrollLines.mock.calls.length).toBe(atCancel);
  });

  it("cancels a running fling the moment wheel input arrives", async () => {
    const { host } = await mountTerminal();

    host.dispatchEvent(makeTouch("touchstart", 200));
    host.dispatchEvent(makeTouch("touchmove", 140));
    const dragCalls = scrollLines.mock.calls.length;
    host.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));
    await waitFor(() => {
      expect(scrollLines.mock.calls.length).toBeGreaterThan(dragCalls);
    });

    // Wheel input wins over momentum: the fling stops after the wheel's own
    // synchronous scroll and never adds another frame.
    host.dispatchEvent(
      new WheelEvent("wheel", {
        deltaY: 1,
        deltaMode: WheelEvent.DOM_DELTA_LINE,
        bubbles: true,
        cancelable: true,
      }),
    );
    const atCancel = scrollLines.mock.calls.length;
    await new Promise((resolve) => setTimeout(resolve, 120));
    expect(scrollLines.mock.calls.length).toBe(atCancel);
  });

  it("leaves a stationary tap untouched so it can focus and open the keyboard", async () => {
    const { host } = await mountTerminal();

    host.dispatchEvent(makeTouch("touchstart", 200));
    const move = makeTouch("touchmove", 198); // 2px jitter, below the threshold
    host.dispatchEvent(move);

    expect(scrollLines).not.toHaveBeenCalled();
    expect(move.defaultPrevented).toBe(false);
  });

  it("captures touch drags from terminal child layers before they can be swallowed", async () => {
    const { host } = await mountTerminal();
    const layer = appendTerminalLayer(host);
    layer.addEventListener("touchmove", (event) => event.stopPropagation());

    layer.dispatchEvent(makeTouch("touchstart", 200));
    const move = makeTouch("touchmove", 140);
    layer.dispatchEvent(move);

    expect(linesScrolled()).toBe(3);
    expect(move.defaultPrevented).toBe(true);
  });

  it("intercepts iPhone touch drags from terminal child layers with scrollLines only", async () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn((query: string) => ({
        matches: query.includes("max-width: 767px"),
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    );
    const { host, socket } = await mountTerminal();
    socket?.emit("open");

    const layer = appendTerminalLayer(host);
    layer.addEventListener("touchmove", (event) => event.stopPropagation());

    layer.dispatchEvent(makeTouch("touchstart", 200));
    const move = makeTouch("touchmove", 140);
    layer.dispatchEvent(move);

    expect(move.defaultPrevented).toBe(true);
    expect(linesScrolled()).toBe(3);

    const inputFrames = socket!.send.mock.calls
      .map((call) => JSON.parse(call[0] as string))
      .filter((frame) => frame.type === "input");
    expect(inputFrames).toHaveLength(0);
  });

  it("uses compact terminal chrome on mobile and desktop", () => {
    // The mobile block covers portrait width AND landscape phones (coarse
    // pointer, short viewport).
    const mobileBlock = terminalRawViewSource.match(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n  \}/,
    );
    expect(mobileBlock).not.toBeNull();
    const mobileCss = mobileBlock![1];

    expect(mobileCss).toContain(".terminal-host");
    // Full-bleed on mobile: the panel meets the screen edges, so side/bottom
    // borders and radii go.
    expect(mobileCss).toMatch(/\.terminal-panel\s*\{[^}]*border-radius:\s*0/);
    expect(mobileCss).toMatch(/\.terminal-panel\s*\{[^}]*border-left:\s*none/);
    expect(mobileCss).toMatch(/\.terminal-host\s*\{[^}]*padding:\s*4px/);
    expect(mobileCss).toMatch(/\.terminal-keys\s*\{[^}]*gap:\s*4px/);
    expect(mobileCss).toMatch(/\.terminal-keys\s*\{[^}]*padding:\s*2px 4px/);
    expect(mobileCss).toMatch(/\.terminal-key\s*\{[^}]*min-height:\s*28px/);
    expect(mobileCss).toMatch(/\.terminal-key\s*\{[^}]*padding:\s*1px 7px/);
    expect(mobileCss).toMatch(/\.terminal-key\s*\{[^}]*font-size:\s*11px/);

    expect(terminalRawViewSource).toMatch(/\.terminal-host\s*\{[^}]*padding:\s*8px/);
    expect(terminalRawViewSource).toMatch(/\.terminal-key\s*\{[^}]*min-height:\s*28px/);
    expect(terminalRawViewSource).toMatch(/@media \(min-width: 768px\)[\s\S]*height:\s*min\(58vh,\s*560px\)/);
  });

  it("does not keep xterm-specific DOM scrollbar styling", () => {
    expect(terminalRawViewSource).not.toContain(".xterm-scrollable-element");
  });

  it("intercepts wheel scroll from terminal child layers into local scrollback", async () => {
    const { host } = await mountTerminal();
    const layer = appendTerminalLayer(host);
    layer.addEventListener("wheel", (event) => event.stopPropagation());

    const wheel = new WheelEvent("wheel", {
      deltaY: 3,
      deltaMode: WheelEvent.DOM_DELTA_LINE,
      bubbles: true,
      cancelable: true,
    });
    layer.dispatchEvent(wheel);

    expect(linesScrolled()).toBe(3);
    expect(wheel.defaultPrevented).toBe(true);
  });
});
