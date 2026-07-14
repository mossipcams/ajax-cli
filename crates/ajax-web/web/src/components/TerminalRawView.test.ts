/// <reference types="vite/client" />

import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, waitFor, queryByRole } from "@testing-library/svelte";
import { tick } from "svelte";
import TerminalRawView from "./TerminalRawView.svelte";
import terminalRawViewSource from "./TerminalRawView.svelte?raw";
import terminalClipboardSource from "../terminalClipboard.ts?raw";
import { fitScale, scaledLogicalRows } from "../terminalGeometry";

const preloadGhosttyRuntime = vi.hoisted(() =>
  vi.fn(() => Promise.resolve({ runtime: "ghostty" })),
);
vi.mock("../terminalPreload", () => ({
  preloadGhosttyRuntime,
}));

const write = vi.fn();
const scrollToBottom = vi.fn();
const scrollLines = vi.fn();
const dispose = vi.fn();
let onDataHandler: ((data: string) => void) | undefined;
const fit = vi.fn();
const fitDispose = vi.fn();

const focus = vi.fn();
const blur = vi.fn();
const reset = vi.fn();
const paste = vi.fn();
const resize = vi.fn();
const getSelection = vi.fn(() => "selected text");
const clearSelection = vi.fn();
let scrollbackLength = 0;
let writeScrollbackGrowth = 0;
let bufferLineText: string | undefined;
let lastTerminal: {
  selectionManager: {
    selectionStart: { col: number; absoluteRow: number } | null;
    selectionEnd: { col: number; absoluteRow: number } | null;
    requestRender: ReturnType<typeof vi.fn>;
  };
  showScrollbar?: () => void;
} | undefined;
let activeSelectionManager: {
  selectionStart: { col: number; absoluteRow: number } | null;
  selectionEnd: { col: number; absoluteRow: number } | null;
  requestRender: ReturnType<typeof vi.fn>;
};
let lastTextarea: HTMLTextAreaElement | undefined;
let customKeyHandler: ((event: KeyboardEvent) => boolean | undefined) | undefined;
let terminalOptions: unknown;
let liveOptions: { fontSize?: number } | undefined;
let onScrollHandler: ((viewportY: number) => void) | undefined;
let viewportY = 0;
let proposedDimensions: { cols: number; rows: number } | undefined;
let terminalHostClientWidth: number | undefined;
let terminalCellMetrics = { width: 8, height: 18 };
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
    renderer = {
      getMetrics: () => terminalCellMetrics,
    };
    buffer = {
      active: {
        viewportY: 0,
        baseY: 0,
        getLine: () =>
          bufferLineText !== undefined
            ? { translateToString: () => bufferLineText }
            : undefined,
      },
    };
    loadAddon = vi.fn();
    open = vi.fn((parent: HTMLElement) => {
      // Match ghostty-web: open() assigns this.element = parent.
      this.element = parent;
      const host = parent.closest(".terminal-host") ?? parent;
      if (terminalHostClientWidth !== undefined) {
        Object.defineProperty(host, "clientWidth", {
          value: terminalHostClientWidth,
          configurable: true,
        });
      }
      parent.appendChild(document.createElement("canvas"));
    });
    // Mimics ghostty-web 0.4.0's writeInternal, which force-scrolls to the
    // bottom on every write while the viewport is away from it
    // (`this.viewportY !== 0 && this.scrollToBottom()`). The component must
    // blind this instance method or every output frame yanks the user out of
    // scrollback.
    write = (data: string | Uint8Array) => {
      write(data);
      scrollbackLength += writeScrollbackGrowth;
      if (viewportY !== 0) this.scrollToBottom();
    };
    scrollToBottom = () => {
      scrollToBottom();
      viewportY = 0;
      onScrollHandler?.(0);
    };
    scrollLines = scrollLines;
    dispose = dispose;
    focus = focus;
    blur = blur;
    reset = reset;
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
    getSelection = getSelection;
    clearSelection = clearSelection;
    getScrollbackLength = () => scrollbackLength;
    attachCustomKeyEventHandler = vi.fn(
      (handler: (event: KeyboardEvent) => boolean | undefined) => {
        customKeyHandler = handler;
      },
    );
    selectionManager!: {
      selectionStart: { col: number; absoluteRow: number } | null;
      selectionEnd: { col: number; absoluteRow: number } | null;
      requestRender: ReturnType<typeof vi.fn>;
    };
    options: { fontSize?: number };
    constructor(options: unknown) {
      this.selectionManager = {
        selectionStart: null,
        selectionEnd: null,
        requestRender: vi.fn(),
      };
      activeSelectionManager = this.selectionManager;
      lastTerminal = this;
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
  customKeyHandler = undefined;
  terminalOptions = undefined;
  onScrollHandler = undefined;
  viewportY = 0;
  proposedDimensions = undefined;
  terminalHostClientWidth = undefined;
  terminalCellMetrics = { width: 8, height: 18 };
  scrollbackLength = 0;
  writeScrollbackGrowth = 0;
  bufferLineText = undefined;
  lastTerminal = undefined;
  liveOptions = undefined;
  getSelection.mockClear();
  getSelection.mockReturnValue("selected text");
  clearSelection.mockClear();
  write.mockClear();
  scrollToBottom.mockClear();
  dispose.mockClear();
  fit.mockClear();
  fitDispose.mockClear();
  focus.mockClear();
  blur.mockClear();
  reset.mockClear();
  paste.mockClear();
  resize.mockClear();
  scrollLines.mockClear();
  ghosttyLoad.mockClear();
  preloadGhosttyRuntime.mockClear();
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
  // A test that toggles keyboard-open/expand and then fails would otherwise
  // leak the class onto <html> and make every later fit test bail.
  document.documentElement.classList.remove("keyboard-open", "terminal-expanded");
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

const inputPayloadsOf = (socket: MockWebSocket) =>
  socket.send.mock.calls
    .map((call) => call[0])
    .filter((payload): payload is ArrayBufferView => ArrayBuffer.isView(payload))
    .map((payload) => new TextDecoder().decode(payload));

const dispatchTextareaBeforeInput = (inputType: string, data: string | null = null) => {
  if (!lastTextarea) throw new Error("terminal textarea was not mounted");
  lastTextarea.dispatchEvent(
    new InputEvent("beforeinput", {
      bubbles: true,
      cancelable: true,
      data,
      inputType,
    }),
  );
};

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
      expect(inputPayloadsOf(socket!)).toEqual(["c", "l", "e", "a", "r", "\r"]);
    });
  });

  it("sends terminal input as binary frames", async () => {
    const { socket } = await mountOpenTerminal();

    onDataHandler?.("a");

    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toContain("a");
    });
  });

  it("forwards successive printable input immediately", async () => {
    const { socket } = await mountOpenTerminal();

    onDataHandler?.("h");
    onDataHandler?.("i");

    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toEqual(expect.arrayContaining(["h", "i"]));
    });
  });

  it("shows printable input locally before the PTY echo returns", async () => {
    const { container, socket } = await mountOpenTerminal();

    onDataHandler?.("h");
    onDataHandler?.("i");

    expect(container.querySelector("[data-testid='terminal-zero-lag-input']")?.textContent).toBe(
      "hi",
    );

    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("hi") }),
    } as MessageEvent);

    await waitFor(() => {
      expect(container.querySelector("[data-testid='terminal-zero-lag-input']")).toBeNull();
    });
  });

  it("shows textarea input locally before Ghostty emits terminal data", async () => {
    const { container, socket } = await mountOpenTerminal();

    dispatchTextareaBeforeInput("insertText", "h");

    expect(container.querySelector("[data-testid='terminal-zero-lag-input']")?.textContent).toBe(
      "h",
    );
    expect(socket?.send).not.toHaveBeenCalled();
  });

  it("tracks terminal font size in the zero-lag overlay inline style", async () => {
    const termFont = 13;
    const { container } = await mountOpenTerminal();

    if (liveOptions) liveOptions.fontSize = termFont;

    const canvas = container.querySelector("canvas") as HTMLElement;
    Object.defineProperty(canvas, "clientWidth", { value: 800, configurable: true });
    Object.defineProperty(canvas, "clientHeight", { value: 480, configurable: true });
    Object.assign(
      (lastTerminal as unknown as { buffer: { active: Record<string, unknown> } }).buffer.active,
      { cursorX: 0, cursorY: 0 },
    );

    dispatchTextareaBeforeInput("insertText", "x");

    const overlay = container.querySelector(
      "[data-testid='terminal-zero-lag-input']",
    ) as HTMLElement;
    expect(overlay).toBeInTheDocument();
    const inlineStyle = overlay.getAttribute("style") ?? "";
    expect(inlineStyle).toContain(`font-size: ${termFont}px`);
    expect(inlineStyle).not.toContain("font-size: 16px");
    expect(liveOptions?.fontSize).toBe(termFont);
  });

  it("keeps optimistic input visible through unrelated output until PTY echo arrives", async () => {
    const { container, socket } = await mountOpenTerminal();

    dispatchTextareaBeforeInput("insertText", "h");
    dispatchTextareaBeforeInput("insertText", "i");

    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("status bar redraw") }),
    } as MessageEvent);

    await waitFor(() => expect(write).toHaveBeenCalledWith("status bar redraw"));
    expect(container.querySelector("[data-testid='terminal-zero-lag-input']")?.textContent).toBe(
      "hi",
    );

    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("hi") }),
    } as MessageEvent);

    await waitFor(() => {
      expect(container.querySelector("[data-testid='terminal-zero-lag-input']")).toBeNull();
    });
  });

  it("renders the optimistic echo above the terminal canvas", () => {
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(\.terminal-zero-lag-input\)\s*\{[^}]*z-index:\s*1/,
    );
  });

  it("does not use flushSync for zero-lag echo", () => {
    expect(terminalRawViewSource).not.toMatch(/\bflushSync\b/);
  });

  it("pins the zero-lag overlay to the cursor without a bottom stretch anchor", async () => {
    // CSS bottom + inline top stretches the overlay into a second terminal.
    const zeroLagCss =
      /\.terminal-host\s+:global\(\.terminal-zero-lag-input\)\s*\{[^}]*\}/;
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(\.terminal-zero-lag-input\)\s*\{[^}]*position:\s*absolute/,
    );
    expect(terminalRawViewSource).not.toMatch(
      /\.terminal-host\s+:global\(\.terminal-zero-lag-input\)\s*\{[^}]*\bbottom\s*:/,
    );
    expect(terminalRawViewSource).not.toMatch(
      /\.terminal-host\s+:global\(\.terminal-zero-lag-input\)\s*\{[^}]*\bleft\s*:/,
    );
    expect(terminalRawViewSource.match(zeroLagCss)?.[0] ?? "").not.toMatch(/\bbottom\s*:/);

    const { container } = await mountOpenTerminal();
    const canvas = container.querySelector("canvas") as HTMLElement;
    Object.defineProperty(canvas, "clientWidth", { value: 800, configurable: true });
    Object.defineProperty(canvas, "clientHeight", { value: 480, configurable: true });
    Object.assign(
      (lastTerminal as unknown as { buffer: { active: Record<string, unknown> } }).buffer.active,
      { cursorX: 3, cursorY: 2 },
    );

    dispatchTextareaBeforeInput("insertText", "x");

    const overlay = container.querySelector(
      "[data-testid='terminal-zero-lag-input']",
    ) as HTMLElement;
    const inlineStyle = overlay.getAttribute("style") ?? "";
    expect(inlineStyle).toMatch(/left:\s*\d/);
    expect(inlineStyle).toMatch(/top:\s*\d/);
    expect(inlineStyle).not.toMatch(/bottom:/);
  });

  it("positions zero-lag overlay with renderer cell metrics", async () => {
    expect(terminalRawViewSource).toMatch(/getMetrics/);

    terminalCellMetrics = { width: 10, height: 16 };
    const { container } = await mountOpenTerminal();
    const canvas = container.querySelector("canvas") as HTMLElement;
    Object.defineProperty(canvas, "clientWidth", { value: 800, configurable: true });
    Object.defineProperty(canvas, "clientHeight", { value: 800, configurable: true });
    Object.assign(
      (lastTerminal as unknown as { buffer: { active: Record<string, unknown> } }).buffer.active,
      { cursorX: 2, cursorY: 5 },
    );

    dispatchTextareaBeforeInput("insertText", "x");

    const overlay = container.querySelector(
      "[data-testid='terminal-zero-lag-input']",
    ) as HTMLElement;
    const inlineStyle = overlay.getAttribute("style") ?? "";
    expect(inlineStyle).toContain("left: 20px");
    expect(inlineStyle).toContain("top: 80px");
  });

  it("does not duplicate optimistic text when Ghostty emits matching data", async () => {
    const { container, socket } = await mountOpenTerminal();

    dispatchTextareaBeforeInput("insertText", "h");
    onDataHandler?.("h");

    expect(container.querySelector("[data-testid='terminal-zero-lag-input']")?.textContent).toBe(
      "h",
    );
    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toContain("h");
    });
  });

  it("updates textarea optimistic input for backspace and enter", async () => {
    const { container } = await mountOpenTerminal();

    dispatchTextareaBeforeInput("insertText", "h");
    dispatchTextareaBeforeInput("insertText", "i");
    dispatchTextareaBeforeInput("deleteContentBackward");

    expect(container.querySelector("[data-testid='terminal-zero-lag-input']")?.textContent).toBe(
      "h",
    );

    dispatchTextareaBeforeInput("insertLineBreak");

    expect(container.querySelector("[data-testid='terminal-zero-lag-input']")).toBeNull();
  });

  it("sends Enter as a raw terminal frame", async () => {
    const { socket } = await mountOpenTerminal();

    onDataHandler?.("\r");

    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toContain("\r");
    });
  });

  it("always forwards backspace to the PTY", async () => {
    const { socket } = await mountOpenTerminal();

    onDataHandler?.("\x7f");

    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toContain("\x7f");
    });
  });

  it("skips Ghostty Backspace keydown so iOS can key-repeat", async () => {
    await mountOpenTerminal();

    await waitFor(() => {
      expect(customKeyHandler).toBeDefined();
    });

    expect(customKeyHandler!({ key: "Backspace" } as KeyboardEvent)).toBe(false);
    expect(customKeyHandler!({ key: "Delete" } as KeyboardEvent)).toBe(false);
    expect(customKeyHandler!({ key: "a" } as KeyboardEvent)).toBeUndefined();
  });

  it("seeds a zero-width space in the textarea so iOS backspace can repeat", async () => {
    await mountOpenTerminal();

    await waitFor(() => {
      expect(lastTextarea).toBeDefined();
      expect(lastTextarea!.value).toContain("\u200B");
    });

    lastTextarea!.value = "";
    dispatchTextareaBeforeInput("deleteContentBackward");
    expect(lastTextarea!.value).toContain("\u200B");
  });

  it("attaches Backspace skip handler and ZWS sentinel for iOS key-repeat", () => {
    expect(terminalRawViewSource).toMatch(/attachCustomKeyEventHandler/);
    expect(terminalRawViewSource).toMatch(/key === ["']Backspace["']/);
    expect(terminalRawViewSource).toMatch(/\\u200B/);
  });

  it("loads ghostty-web with the served wasm asset", async () => {
    await mountTerminal();

    expect(preloadGhosttyRuntime).toHaveBeenCalled();
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

  it("owns status, paste/copy fallbacks, and key bar under bottom controls", async () => {
    delete (navigator as { clipboard?: unknown }).clipboard;
    Object.defineProperty(document, "execCommand", {
      value: vi.fn().mockReturnValue(false),
      configurable: true,
    });
    const { getByTestId, getByRole, host } = await mountOpenTerminal();

    const bottom = getByTestId("terminal-bottom-controls");
    expect(bottom.querySelector('[data-testid="terminal-status"]')).toBeInTheDocument();
    expect(bottom.querySelector('[role="toolbar"][aria-label="Terminal keys"]')).toBeInTheDocument();

    getByRole("button", { name: "Paste" }).click();
    await waitFor(() => {
      expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument();
    });
    expect(bottom.contains(getByTestId("terminal-paste-fallback"))).toBe(true);

    stubTerminalCanvas(host);
    vi.useFakeTimers();
    host.dispatchEvent(makeTouch("touchstart", 105, 105));
    vi.advanceTimersByTime(500);
    host.dispatchEvent(makeTouch("touchmove", 105, 305));
    host.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));
    await vi.advanceTimersByTimeAsync(0);
    expect(getByTestId("terminal-copy-overlay")).toBeInTheDocument();
    vi.useRealTimers();
    getByRole("button", { name: "Copy" }).click();
    await waitFor(() => {
      expect(getByTestId("terminal-copy-fallback")).toBeInTheDocument();
    });
    expect(bottom.contains(getByTestId("terminal-copy-fallback"))).toBe(true);

    // Source contract: fallbacks are normal-flow chrome, not absolute overlays.
    const pasteFallbackCss = terminalRawViewSource.match(
      /\.terminal-paste-fallback\s*\{[^}]*\}/,
    );
    expect(pasteFallbackCss?.[0] ?? "").not.toMatch(/position:\s*absolute/);
    expect(terminalRawViewSource).toMatch(
      /\.terminal-new-output\s*\{[^}]*position:\s*absolute/,
    );
    // Expanded copy overlay mirrors expand-corner safe-area offsets.
    expect(terminalRawViewSource).toMatch(
      /\.terminal-panel\.is-expanded\s+\.terminal-copy-overlay\s*\{[^}]*env\(safe-area-inset-top\)/,
    );
    expect(terminalRawViewSource).toMatch(
      /\.terminal-panel\.is-expanded\s+\.terminal-copy-overlay\s*\{[^}]*env\(safe-area-inset-right\)/,
    );
  });

  it("renders a debug placeholder without ghostty when localStorage flag is set", () => {
    localStorage.setItem("ajax.debug.terminalPlaceholder", "true");
    ghosttyLoad.mockClear();
    preloadGhosttyRuntime.mockClear();
    const { getByTestId, container } = render(TerminalRawView, { props: { handle: "web/fix-login" } });
    expect(getByTestId("terminal-placeholder")).toBeInTheDocument();
    expect(container.querySelector("canvas")).toBeNull();
    expect(getByTestId("task-terminal-panel")).toHaveAttribute("data-terminal-engine", "placeholder");
    expect(preloadGhosttyRuntime).not.toHaveBeenCalled();
  });

  it("inserts an inline spacer while expanded to preserve route scroll extent", async () => {
    localStorage.setItem("ajax.debug.terminalPlaceholder", "true");
    const { getByRole, container } = render(TerminalRawView, { props: { handle: "web/fix-login" } });
    getByRole("button", { name: "Expand terminal" }).click();
    await tick();
    expect(container.querySelector(".terminal-inline-spacer")).toBeInTheDocument();
    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(true);
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
    expect(terminalRawViewSource).toMatch(/\.terminal-panel\s*\{[^}]*min-width:\s*0/);
    expect(terminalRawViewSource).toMatch(/\.terminal-panel\s*\{[^}]*max-width:\s*100%/);
    expect(terminalRawViewSource).toMatch(/\.terminal-host\s*\{[^}]*flex:\s*1 1 auto/);
    expect(terminalRawViewSource).toMatch(/\.terminal-host\s*\{[^}]*min-height:\s*0/);
    expect(terminalRawViewSource).toMatch(/\.terminal-host\s*\{[^}]*min-width:\s*0/);
    expect(terminalRawViewSource).toMatch(/\.terminal-host\s*\{[^}]*width:\s*100%/);
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

  it("holds the reading position steady when writes grow the scrollback", async () => {
    // viewportY is measured from the bottom of the buffer, so when output
    // pushes new lines into scrollback the view must step back by the same
    // amount or the text the user is reading crawls upward.
    const { socket } = await mountOpenTerminal();
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    await settleFrames();

    scrollbackLength = 40;
    scrollAwayFromBottom();
    scrollLines.mockClear();
    writeScrollbackGrowth = 2;

    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("two new lines") }),
    } as MessageEvent);

    await waitFor(() => expect(write).toHaveBeenCalledWith("two new lines"));
    expect(linesScrolled()).toBe(-2);
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

    proposedDimensions = { cols: 90, rows: 28 };
    dispatchVisualViewport("resize");

    vi.advanceTimersByTime(20);
    expect(resize).toHaveBeenCalledWith(90, 28);
    expect(socket?.send).not.toHaveBeenCalled();

    vi.advanceTimersByTime(79);
    expect(socket?.send).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(socket?.send).toHaveBeenCalledWith(
      JSON.stringify({ type: "resize", cols: 90, rows: 28 }),
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
    proposedDimensions = { cols: 72, rows: 20 };
    setKeyboardOpen(false);
    dispatchVisualViewport("resize");
    vi.advanceTimersByTime(100);

    expect(resizeFrames()).toHaveLength(1);
    expect(resizeFrames()[0]).toEqual({ type: "resize", cols: 80, rows: 20 });
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
    expect(socket?.send).toHaveBeenCalledTimes(1);
    expect(socket?.send).toHaveBeenCalledWith(
      JSON.stringify({ type: "resize", cols: 80, rows: 24 }),
    );
  });

  it("uses agent-sized floor of 80 columns on a narrow host", async () => {
    terminalHostClientWidth = 390;
    proposedDimensions = { cols: 43, rows: 30 };
    const { socket } = await mountTerminal();

    socket?.emit("open");

    await waitFor(() => {
      const [cols] = resize.mock.calls.at(-1) ?? [];
      expect(cols).toBeGreaterThanOrEqual(80);
      expect(lastTerminal?.element.style.transform).toMatch(/scale\(/);
    });
    const expectedRows = scaledLogicalRows(30, fitScale(390, 80, 8));
    expect(resizeFramesOf(socket!)).toContainEqual({ type: "resize", cols: 80, rows: expectedRows });
    expect(expectedRows).toBe(50);
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

  it("resets the terminal buffer and snaps to bottom on reconnect", async () => {
    const { socket: first } = await mountOpenTerminal();
    await settleFrames();

    first?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("session output") }),
    } as MessageEvent);
    await waitFor(() => expect(write).toHaveBeenCalledWith("session output"));

    vi.useFakeTimers();
    reset.mockClear();
    scrollToBottom.mockClear();
    first!.readyState = MockWebSocket.CLOSED;
    first?.emit("close");
    await vi.advanceTimersByTimeAsync(1000);

    const second = MockWebSocket.instances[1];
    second?.emit("open");
    await vi.advanceTimersByTimeAsync(1000);

    expect(reset).toHaveBeenCalled();
    expect(scrollToBottom).toHaveBeenCalled();
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

  it("names paste fallback state transitions", () => {
    expect(terminalClipboardSource).toContain("openPasteFallback");
    expect(terminalClipboardSource).toContain("closePasteFallback");
    expect(terminalClipboardSource).toContain("takePasteFallbackText");
    expect(terminalClipboardSource).toContain("createTerminalClipboardUi");
    expect(terminalRawViewSource).toContain("sendPasteFallbackText");
    expect(terminalRawViewSource).toContain("requestPaste");
    expect(terminalRawViewSource).toContain('data-testid="terminal-paste-fallback"');
    expect(terminalRawViewSource).not.toContain("if (text) term?.paste(text)");
  });

  it("surfaces a clipboard read failure instead of silently doing nothing", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: vi.fn().mockRejectedValue(new Error("denied")) },
      configurable: true,
    });
    const { getByRole, getByTestId } = await mountOpenTerminal();

    getByRole("button", { name: "Paste" }).click();

    await waitFor(() => {
      expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument();
    });
    expect(paste).not.toHaveBeenCalled();
  });

  it("sends paste fallback textarea value through term.paste and closes the tray", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: vi.fn().mockRejectedValue(new Error("denied")) },
      configurable: true,
    });
    const { getByRole, getByTestId, queryByTestId } = await mountOpenTerminal();

    getByRole("button", { name: "Paste" }).click();
    await waitFor(() => {
      expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument();
    });

    const textarea = getByTestId("terminal-paste-fallback").querySelector("textarea")!;
    textarea.value = "hello from tray";
    getByRole("button", { name: "Send" }).click();

    await waitFor(() => {
      expect(paste).toHaveBeenCalledWith("hello from tray");
      expect(focus).toHaveBeenCalled();
    });
    await tick();
    expect(queryByTestId("terminal-paste-fallback")).not.toBeInTheDocument();
  });

  it("does not paste when Send is tapped with an empty fallback value", async () => {
    delete (navigator as { clipboard?: unknown }).clipboard;
    const { getByRole, getByTestId, queryByTestId } = await mountOpenTerminal();

    getByRole("button", { name: "Paste" }).click();
    await waitFor(() => {
      expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument();
    });

    getByRole("button", { name: "Send" }).click();

    expect(paste).not.toHaveBeenCalled();
    await tick();
    expect(queryByTestId("terminal-paste-fallback")).not.toBeInTheDocument();
  });

  it("sends an Escape byte when the Esc key is tapped", async () => {
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: "Esc" }).click();

    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toContain("\x1b");
    });
  });

  it("folds the next letter into a control code after Ctrl is armed", async () => {
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: "Ctrl" }).click();
    onDataHandler?.("c");

    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toContain("\x03");
    });
  });

  it("auto-disarms sticky Ctrl after the timeout so a later key is unmodified", async () => {
    vi.useFakeTimers();
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: /Ctrl/ }).click();
    vi.advanceTimersByTime(4000);

    onDataHandler?.("c");

    // The arm expired, so "c" is sent literally, not folded to \x03.
    expect(inputPayloadsOf(socket!)).toContain("c");
    expect(inputPayloadsOf(socket!)).not.toContain("\x03");
    vi.useRealTimers();
  });

  it("sends Enter unchanged when Ctrl is armed", async () => {
    // Ctrl folds letters and cursor keys but leaves Enter as a plain "\r" —
    // which must still take the normal Enter path.
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: /Ctrl/ }).click();
    onDataHandler?.("\r");

    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toContain("\r");
    });
  });

  it("applies an armed Ctrl to a control-bar cursor key, then disarms", async () => {
    const { getByRole, socket } = await mountOpenTerminal();

    getByRole("button", { name: /Ctrl/ }).click();
    getByRole("button", { name: "←" }).click();

    // Ctrl+← becomes the Ctrl-modified CSI cursor sequence.
    await waitFor(() => {
      expect(inputPayloadsOf(socket!)).toContain("\x1b[1;5D");
    });

    // The arm was consumed: a following key is unmodified.
    socket!.send.mockClear();
    onDataHandler?.("x");
    expect(inputPayloadsOf(socket!)).toContain("x");
  });

  it("snaps the visible viewport on expand while the keyboard is open: document top, host bottom crop, terminal scroll bottom", async () => {
    vi.useFakeTimers();
    const { getByRole, host } = await mountOpenTerminal();
    vi.advanceTimersByTime(400);
    setKeyboardOpen(true);

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
    setKeyboardOpen(false);
    vi.useRealTimers();
  });

  it("focuses fullscreen without bottom-cropping the terminal before the keyboard opens", async () => {
    vi.useFakeTimers();
    const { getByRole, host } = await mountOpenTerminal();
    vi.advanceTimersByTime(400);

    scrollAwayFromBottom();
    scrollToBottom.mockClear();

    Object.defineProperty(host, "scrollHeight", { value: 800, configurable: true });
    Object.defineProperty(host, "clientHeight", { value: 300, configurable: true });
    host.scrollTop = 120;
    const focusSpy = vi.spyOn(lastTextarea!, "focus");

    getByRole("button", { name: "Expand terminal" }).click();
    await tick();

    expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
    expect(host.scrollTop).toBe(120);
    expect(scrollToBottom).not.toHaveBeenCalled();
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
    const focusSpy = vi.spyOn(lastTextarea!, "focus");

    getByRole("button", { name: "Expand terminal" }).click();
    await tick();

    expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
    expect(focus).not.toHaveBeenCalled();
    expect(blur).not.toHaveBeenCalled();
  });

  it("blurs the terminal when exiting fullscreen so iOS closes the keyboard", async () => {
    const { getByRole } = await mountOpenTerminal();
    const toggle = getByRole("button", { name: "Expand terminal" });
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    focus.mockClear();
    blur.mockClear();
    const blurSpy = vi.spyOn(lastTextarea!, "blur");

    toggle.click();
    await tick();
    toggle.click();
    await tick();

    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(false);
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(blurSpy).toHaveBeenCalledTimes(1);
    expect(blur).toHaveBeenCalledTimes(1);
  });

  it("keeps the terminal hotkey row compact", () => {
    expect(terminalRawViewSource).toMatch(/\.terminal-keys\s*\{[^}]*gap:\s*4px/);
    expect(terminalRawViewSource).toMatch(/\.terminal-keys\s*\{[^}]*padding:\s*2px 4px/);
    expect(terminalRawViewSource).toMatch(/\.terminal-key\s*\{[^}]*min-height:\s*28px/);
    expect(terminalRawViewSource).toMatch(/\.terminal-key\s*\{[^}]*font-size:\s*11px/);
  });

  it("drops safe-area bottom pad on bottom controls while keyboard is open", () => {
    expect(terminalRawViewSource).toMatch(
      /:global\(html\.keyboard-open\)\s+\.terminal-bottom-controls\s*\{[^}]*padding-bottom:\s*6px/,
    );
  });

  it("collapses empty terminal status while keyboard is open", () => {
    expect(terminalRawViewSource).toMatch(
      /:global\(html\.keyboard-open\)\s+\.terminal-status\.is-empty\s*\{[^}]*display:\s*none/,
    );
  });

  it("does not center the canvas in the host", () => {
    expect(terminalRawViewSource).not.toMatch(
      /:global\(\.terminal-panel canvas\)\s*\{[^}]*margin-inline:\s*auto/,
    );
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

  it("resizes the grid on expand even while the keyboard is open", async () => {
    proposedDimensions = { cols: 55, rows: 30 };
    const { getByRole, socket } = await mountOpenTerminal();
    await settleFrames();
    socket!.send.mockClear();
    resize.mockClear();

    document.documentElement.classList.add("keyboard-open");
    getByRole("button", { name: "Expand terminal" }).click();
    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(true);
    proposedDimensions = { cols: 55, rows: 60 };
    await settleFrames();
    await settleFrames();

    await waitFor(() => expect(resize).toHaveBeenCalledWith(80, 60));
    expect(resizeFramesOf(socket!)).toContainEqual({ type: "resize", cols: 80, rows: 60 });
    document.documentElement.classList.remove("keyboard-open");
  });

  it("keeps expand flush through the settle window while the keyboard is open", async () => {
    vi.useFakeTimers();
    proposedDimensions = { cols: 55, rows: 30 };
    const { getByRole, socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400);
    socket!.send.mockClear();
    resize.mockClear();

    document.documentElement.classList.add("keyboard-open");
    getByRole("button", { name: "Expand terminal" }).click();
    proposedDimensions = { cols: 55, rows: 60 };
    vi.advanceTimersByTime(50);

    socket!.send.mockClear();
    resize.mockClear();
    proposedDimensions = { cols: 80, rows: 90 };
    vi.advanceTimersByTime(300);

    expect(resize).toHaveBeenCalledWith(80, 90);
    expect(resizeFramesOf(socket!)).toContainEqual({ type: "resize", cols: 80, rows: 90 });
    document.documentElement.classList.remove("keyboard-open");
    vi.useRealTimers();
  });

  it("re-fits again after the expand viewport settles", async () => {
    vi.useFakeTimers();
    proposedDimensions = { cols: 55, rows: 60 };
    const { getByRole, socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400);
    socket!.send.mockClear();

    getByRole("button", { name: "Expand terminal" }).click();
    vi.advanceTimersByTime(50);

    proposedDimensions = { cols: 55, rows: 90 };
    socket!.send.mockClear();
    vi.advanceTimersByTime(300);

    expect(resizeFramesOf(socket!)).toContainEqual({ type: "resize", cols: 80, rows: 90 });
    vi.useRealTimers();
  });

  it("disables autocorrect/autocapitalize on the ghostty input", async () => {
    render(TerminalRawView, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(lastTextarea?.getAttribute("autocapitalize")).toBe("off");
      expect(lastTextarea?.getAttribute("autocorrect")).toBe("off");
      expect(lastTextarea?.getAttribute("spellcheck")).toBe("false");
      expect(lastTextarea?.style.fontSize).toBe("16px");
    });
  });

  it("uses a readable font size on a mobile viewport", async () => {
    // 13px matches desktop: the old 10px default was a column-count lever
    // that fit geometry made obsolete.
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

  function scaleLayerTransform(container: HTMLElement): string {
    const layer = container.querySelector(".terminal-scale-layer") as HTMLElement;
    return layer?.style.transform ?? "";
  }

  function stubCellHeight(host: HTMLElement, cellPx = 18) {
    const canvas = host.querySelector("canvas") as HTMLElement;
    if (!canvas) throw new Error("terminal canvas missing");
    Object.defineProperty(canvas, "clientHeight", {
      value: cellPx * 24,
      configurable: true,
    });
  }

  it("applies a sub-cell translate on the scale layer during touch drag", async () => {
    scrollbackLength = 40;
    const { host, container } = await mountOpenTerminal();
    viewportY = 3;
    onScrollHandler?.(3);
    await waitFor(() => expect(lastTerminal).toBeDefined());
    stubCellHeight(host, 18);
    await settleFrames();

    host.dispatchEvent(makeTouch("touchstart", 200, 10));
    const move = makeTouch("touchmove", 190, 10);
    host.dispatchEvent(move);

    expect(move.defaultPrevented).toBe(true);
    expect(scaleLayerTransform(container)).toContain("translateY(-10px)");
    expect(scrollLines).not.toHaveBeenCalled();
  });

  it("clears the sub-cell translate on touchend before fling frames run", async () => {
    scrollbackLength = 40;
    const { host, container } = await mountOpenTerminal();
    viewportY = 3;
    onScrollHandler?.(3);
    await waitFor(() => expect(lastTerminal).toBeDefined());
    stubCellHeight(host, 18);
    await settleFrames();

    host.dispatchEvent(makeTouch("touchstart", 200));
    host.dispatchEvent(makeTouch("touchmove", 190));
    expect(scaleLayerTransform(container)).toContain("translateY(-10px)");
    host.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));

    const transform = scaleLayerTransform(container);
    expect(transform).not.toMatch(/translateY\([^0]/);
  });

  it("clears a nonzero sub-cell offset when pinned output arrives", async () => {
    scrollbackLength = 40;
    const { host, container, socket } = await mountOpenTerminal();
    await waitFor(() => expect(lastTerminal).toBeDefined());
    stubCellHeight(host, 18);
    await settleFrames();

    host.dispatchEvent(makeTouch("touchstart", 200, 10));
    host.dispatchEvent(makeTouch("touchmove", 210, 10));
    expect(scaleLayerTransform(container)).toContain("translateY(10px)");

    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("fresh line") }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("fresh line");
      expect(scaleLayerTransform(container)).not.toMatch(/translateY\([^0]/);
    });
  });

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

  it("keeps the readable font when fit geometry already fits", async () => {
    proposedDimensions = { cols: 48, rows: 30 };
    await mountTerminal();

    await waitFor(() => {
      expect(liveOptions?.fontSize).toBe(13);
    });
    // The auto-fit is not an operator choice and must not overwrite one.
    expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBeNull();
  });

  it("uses the clipped host width when Ghostty proposes the current grid", async () => {
    // Real ghostty-web measures the terminal host. After Ajax has resized that
    // host to the column floor, proposeDimensions can report the current
    // floor instead of the phone-visible width. The cap must still use the
    // clipped host width, where 384px holds 48 cells at the 13px font.
    terminalHostClientWidth = 384;
    proposedDimensions = { cols: 80, rows: 30 };
    await mountTerminal();

    await waitFor(() => {
      expect(liveOptions?.fontSize).toBe(13);
    });
    expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBeNull();
  });

  it("restores the operator's font once a wider viewport fits it again", async () => {
    vi.useFakeTimers();
    proposedDimensions = { cols: 48, rows: 30 };
    const { socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400);
    expect(liveOptions?.fontSize).toBe(13);
    socket!.send.mockClear();

    // Rotate to a viewport with room to spare: the font climbs back to the
    // operator's (default) 13px choice — not to the 20px pinch ceiling.
    proposedDimensions = { cols: 200, rows: 30 };
    window.dispatchEvent(new Event("resize"));
    vi.advanceTimersByTime(200);

    expect(liveOptions?.fontSize).toBe(13);
    expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBeNull();
    vi.useRealTimers();
  });

  it("caps a pinch spread at the size where the 80-column floor still fits", async () => {
    // 100 columns fit at 13px, so 80 columns fit up to floor(13 * 100 / 80) = 16.
    proposedDimensions = { cols: 100, rows: 30 };
    const { host } = await mountTerminal();

    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 100, y: 100 },
        { x: 200, y: 100 },
      ]),
    );
    host.dispatchEvent(
      makePinch("touchmove", [
        { x: 50, y: 100 },
        { x: 250, y: 100 },
      ]),
    );

    expect(liveOptions?.fontSize).toBe(16);
    expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBe("16");
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

  it("grows the font on pinch spread in fit mode", async () => {
    const { host } = await mountTerminal();

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

    expect(move.defaultPrevented).toBe(true);
    expect(liveOptions?.fontSize).toBe(20);
    expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBe("20");
    expect(scrollLines).not.toHaveBeenCalled();
  });

  it("fits columns to the full host width with agent-sized floor", async () => {
    terminalHostClientWidth = 384;
    proposedDimensions = { cols: 46, rows: 30 };
    const { socket } = await mountTerminal();

    socket?.emit("open");

    await waitFor(() => {
      const expectedRows = scaledLogicalRows(30, fitScale(384, 80, 8));
      expect(resize).toHaveBeenCalledWith(80, expectedRows);
    });
  });

  it("hides the scrollbar via the scrollbarWidth option", async () => {
    await mountTerminal();

    expect((terminalOptions as { scrollbarWidth?: number }).scrollbarWidth).toBe(0);
  });

  it("passes a mobile-aware scrollback limit into Ghostty", async () => {
    stubMatchMedia(() => false);
    await mountTerminal();

    expect((terminalOptions as { scrollback?: number }).scrollback).toBe(10_000);
  });

  it("passes the mobile scrollback limit when the mobile media heuristic matches", async () => {
    stubMatchMedia(
      (query) =>
        query === "(max-width: 767px), (pointer: coarse) and (max-height: 500px)",
    );
    await mountTerminal();

    expect((terminalOptions as { scrollback?: number }).scrollback).toBe(2000);
  });

  it("disables ghostty-web smooth scrolling so viewportY stays instant", async () => {
    await mountTerminal();

    expect((terminalOptions as { smoothScrollDuration?: number }).smoothScrollDuration).toBe(0);
  });

  function stubTerminalCanvas(host: HTMLElement) {
    const canvas = host.querySelector("canvas");
    if (!canvas) throw new Error("terminal canvas missing");
    vi.spyOn(canvas, "getBoundingClientRect").mockReturnValue({
      left: 0,
      top: 0,
      width: 800,
      height: 480,
      right: 800,
      bottom: 480,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
  }

  it("copies the selection after a long-press drag", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    const { host, getByTestId, getByRole, queryByTestId } = await mountTerminal();
    stubTerminalCanvas(host);

    vi.useFakeTimers();
    host.dispatchEvent(makeTouch("touchstart", 105, 105));
    vi.advanceTimersByTime(500);

    expect(activeSelectionManager.selectionStart).toEqual({ col: 10, absoluteRow: 5 });

    const move = makeTouch("touchmove", 105, 305);
    host.dispatchEvent(move);

    expect(move.defaultPrevented).toBe(true);
    expect(scrollLines).not.toHaveBeenCalled();
    expect(activeSelectionManager.selectionEnd).toEqual({ col: 30, absoluteRow: 5 });

    const end = new Event("touchend", { bubbles: true, cancelable: true });
    const stopPropagation = vi.spyOn(end, "stopPropagation");
    host.dispatchEvent(end);

    await vi.advanceTimersByTimeAsync(0);
    expect(writeText).not.toHaveBeenCalled();
    expect(clearSelection).not.toHaveBeenCalled();
    expect(stopPropagation).toHaveBeenCalled();
    expect(getByTestId("terminal-copy-overlay")).toBeInTheDocument();
    expect(getByRole("button", { name: "Copy" })).toBeInTheDocument();
    vi.useRealTimers();

    getByRole("button", { name: "Copy" }).click();
    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith("selected text");
    });
    expect(clearSelection).toHaveBeenCalled();
    expect(getByTestId("terminal-status").textContent).toContain("Copied");
    expect(queryByTestId("terminal-copy-overlay")).not.toBeInTheDocument();
  });

  it("opens a readonly copy fallback when clipboard write fails", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
      configurable: true,
    });
    Object.defineProperty(document, "execCommand", {
      value: vi.fn().mockReturnValue(false),
      configurable: true,
    });
    const { host, getByTestId, getByRole, queryByTestId } = await mountTerminal();
    stubTerminalCanvas(host);

    vi.useFakeTimers();
    host.dispatchEvent(makeTouch("touchstart", 105, 105));
    vi.advanceTimersByTime(500);
    host.dispatchEvent(makeTouch("touchmove", 105, 305));
    host.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));
    await vi.advanceTimersByTimeAsync(0);
    expect(getByTestId("terminal-copy-overlay")).toBeInTheDocument();
    vi.useRealTimers();

    getByRole("button", { name: "Copy" }).click();
    await waitFor(() => {
      expect(getByTestId("terminal-copy-fallback")).toBeInTheDocument();
    });
    expect(queryByTestId("terminal-copy-overlay")).not.toBeInTheDocument();
    const fallback = getByTestId("terminal-copy-fallback");
    const textarea = fallback.querySelector("textarea")!;
    expect(textarea).toBeInTheDocument();
    expect(textarea.readOnly).toBe(true);
    expect(textarea.value).toBe("selected text");
    expect(textarea.selectionStart).toBe(0);
    expect(textarea.selectionEnd).toBe("selected text".length);
  });

  it("dismisses copy overlay without copying when selection is cancelled", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    const { host, queryByTestId } = await mountTerminal();
    stubTerminalCanvas(host);

    vi.useFakeTimers();
    host.dispatchEvent(makeTouch("touchstart", 105, 105));
    vi.advanceTimersByTime(500);
    expect(activeSelectionManager.selectionStart).toEqual({ col: 10, absoluteRow: 5 });

    // Second finger during a live selection cancels via endSelection(true).
    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 105, y: 105 },
        { x: 205, y: 105 },
      ]),
    );
    await vi.advanceTimersByTimeAsync(0);
    expect(writeText).not.toHaveBeenCalled();
    expect(clearSelection).toHaveBeenCalled();
    expect(queryByTestId("terminal-copy-overlay")).not.toBeInTheDocument();
    vi.useRealTimers();
  });

  it("selects the word under a bare long-press", async () => {
    bufferLineText = "hello world ok";
    const { host, getByTestId } = await mountTerminal();
    stubTerminalCanvas(host);

    vi.useFakeTimers();
    host.dispatchEvent(makeTouch("touchstart", 5, 75));
    vi.advanceTimersByTime(500);

    expect(activeSelectionManager.selectionStart).toEqual({ col: 6, absoluteRow: 0 });
    expect(activeSelectionManager.selectionEnd).toEqual({ col: 10, absoluteRow: 0 });

    host.dispatchEvent(new Event("touchend", { bubbles: true, cancelable: true }));
    await vi.advanceTimersByTimeAsync(0);
    expect(getByTestId("terminal-copy-overlay")).toBeInTheDocument();
    expect(clearSelection).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("cancels a long-press when the finger scrolls before the timeout", async () => {
    const { host } = await mountTerminal();
    stubTerminalCanvas(host);

    vi.useFakeTimers();
    host.dispatchEvent(makeTouch("touchstart", 200, 105));
    host.dispatchEvent(makeTouch("touchmove", 140, 105));
    vi.advanceTimersByTime(500);

    expect(activeSelectionManager.selectionStart).toBeNull();
    expect(scrollLines).toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("opens a paste fallback sheet when the async clipboard API is unavailable", async () => {
    delete (navigator as { clipboard?: unknown }).clipboard;
    const { getByRole, getByTestId, queryByTestId } = await mountOpenTerminal();

    getByRole("button", { name: "Paste" }).click();

    await waitFor(() => {
      expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument();
    });
    const sheet = getByTestId("terminal-paste-fallback");
    const textarea = sheet.querySelector("textarea");
    expect(textarea).toBeInTheDocument();

    const pasteEvent = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(pasteEvent, "clipboardData", {
      value: { getData: () => "pasted text" },
    });
    textarea!.dispatchEvent(pasteEvent);

    expect(paste).toHaveBeenCalledWith("pasted text");
    await tick();
    expect(queryByTestId("terminal-paste-fallback")).not.toBeInTheDocument();
  });

  it("closes the paste fallback sheet without pasting when Cancel is tapped", async () => {
    delete (navigator as { clipboard?: unknown }).clipboard;
    const { getByRole, getByTestId, queryByTestId } = await mountOpenTerminal();

    getByRole("button", { name: "Paste" }).click();
    await waitFor(() => {
      expect(getByTestId("terminal-paste-fallback")).toBeInTheDocument();
    });

    getByRole("button", { name: "Cancel" }).click();

    expect(paste).not.toHaveBeenCalled();
    await tick();
    expect(queryByTestId("terminal-paste-fallback")).not.toBeInTheDocument();
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

  it("lets pinch shrink from the clipped host width when Ghostty proposes the floor grid", async () => {
    terminalHostClientWidth = 384;
    proposedDimensions = { cols: 80, rows: 30 };
    const { host } = await mountTerminal();

    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 100, y: 100 },
        { x: 200, y: 100 },
      ]),
    );
    host.dispatchEvent(
      makePinch("touchmove", [
        { x: 100, y: 100 },
        { x: 180, y: 100 },
      ]),
    );

    expect(liveOptions?.fontSize).toBe(10);
    expect(window.localStorage.getItem("ajax.terminal.fontSize")).toBe("10");
  });

  it("clamps horizontal pan after pinch refit shrinks the canvas", async () => {
    const { host } = await mountTerminal();
    sizeHostForPan(host, 900, 338);
    host.scrollLeft = 500;

    vi.useFakeTimers();
    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 100, y: 100 },
        { x: 200, y: 100 },
      ]),
    );
    host.dispatchEvent(
      makePinch("touchmove", [
        { x: 75, y: 100 },
        { x: 225, y: 100 },
      ]),
    );

    sizeHostForPan(host, 480, 338);

    await vi.advanceTimersByTimeAsync(50);
    vi.advanceTimersByTime(300);
    await tick();

    expect(host.scrollLeft).toBe(142);
    vi.useRealTimers();
  });

  it("resizes the PTY to agent-sized 80 columns in fit mode", async () => {
    proposedDimensions = { cols: 48, rows: 30 };
    const { socket } = await mountTerminal();

    socket?.emit("open");

    await waitFor(() => {
      expect(resize).toHaveBeenCalledWith(80, 30);
      expect(liveOptions?.fontSize).toBe(13);
      expect(resizeFramesOf(socket!)).toContainEqual({ type: "resize", cols: 80, rows: 30 });
    });
  });

  it("floors fit mode at 80 columns", async () => {
    proposedDimensions = { cols: 12, rows: 30 };
    const { socket } = await mountTerminal();

    socket?.emit("open");

    await waitFor(() => {
      expect(resize).toHaveBeenCalledWith(80, 30);
    });
  });

  it("does not render the dead Wide geometry hotkey", async () => {
    const { queryByRole } = await mountOpenTerminal();

    expect(queryByRole("button", { name: "Wide" })).not.toBeInTheDocument();
  });

  it("ignores a stale persisted wide geometry mode", async () => {
    window.localStorage.setItem("ajax.terminal.geometryMode", "wide");
    proposedDimensions = { cols: 55, rows: 30 };
    const { socket } = await mountTerminal();

    socket?.emit("open");

    await waitFor(() => {
      expect(resize).toHaveBeenCalledWith(80, 30);
      expect(resize).not.toHaveBeenCalledWith(55, 30);
    });
  });

  it("flushes the PTY resize when the pinch ends", async () => {
    vi.useFakeTimers();
    const { host, socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400); // settle the open-path refits
    socket!.send.mockClear();
    proposedDimensions = { cols: 100, rows: 30 };

    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 100, y: 100 },
        { x: 200, y: 100 },
      ]),
    );
    host.dispatchEvent(
      makePinch("touchmove", [
        { x: 75, y: 100 },
        { x: 225, y: 100 },
      ]),
    );
    host.dispatchEvent(makePinch("touchend", []));

    // Animation frames only — well under the 100ms resize debounce.
    vi.advanceTimersByTime(50);
    expect(resizeFramesOf(socket!)).toContainEqual({ type: "resize", cols: 100, rows: 30 });
    vi.useRealTimers();
  });

  it("applies the pinch rewrap while the keyboard is open", async () => {
    vi.useFakeTimers();
    const { host, socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400); // settle the open-path refits
    setKeyboardOpen(true);
    resize.mockClear();
    socket!.send.mockClear();
    proposedDimensions = { cols: 100, rows: 30 };

    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 100, y: 100 },
        { x: 200, y: 100 },
      ]),
    );
    host.dispatchEvent(
      makePinch("touchmove", [
        { x: 75, y: 100 },
        { x: 225, y: 100 },
      ]),
    );
    host.dispatchEvent(makePinch("touchend", []));

    // Animation frames only — well under the 100ms resize debounce.
    vi.advanceTimersByTime(50);
    expect(document.documentElement.classList.contains("keyboard-open")).toBe(true);
    expect(resize).toHaveBeenCalled();
    expect(resizeFramesOf(socket!)).toContainEqual({ type: "resize", cols: 100, rows: 30 });

    // A later non-pinch refit must still be withheld while the keyboard is open.
    resize.mockClear();
    socket!.send.mockClear();
    dispatchVisualViewport("resize");
    vi.advanceTimersByTime(400);
    expect(document.documentElement.classList.contains("keyboard-open")).toBe(true);
    expect(resizeFramesOf(socket!)).toHaveLength(0);

    setKeyboardOpen(false);
    vi.useRealTimers();
  });

  it("owns the two-finger touchstart on the host", async () => {
    const { host } = await mountTerminal();
    const event = makePinch("touchstart", [
      { x: 100, y: 100 },
      { x: 200, y: 100 },
    ]);
    host.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it("refits again after pinch layout settles to the screen dimensions", async () => {
    vi.useFakeTimers();
    proposedDimensions = { cols: 100, rows: 30 };
    const { host, socket } = await mountOpenTerminal();
    vi.advanceTimersByTime(400);
    resize.mockClear();
    socket!.send.mockClear();

    host.dispatchEvent(
      makePinch("touchstart", [
        { x: 100, y: 100 },
        { x: 200, y: 100 },
      ]),
    );
    host.dispatchEvent(
      makePinch("touchmove", [
        { x: 75, y: 100 },
        { x: 225, y: 100 },
      ]),
    );

    vi.advanceTimersByTime(16);
    expect(resize).toHaveBeenCalledWith(100, 30);

    proposedDimensions = { cols: 100, rows: 42 };
    vi.advanceTimersByTime(16);

    expect(resize).toHaveBeenCalledWith(100, 42);

    vi.advanceTimersByTime(300);
    expect(resizeFramesOf(socket!)).toEqual([{ type: "resize", cols: 100, rows: 42 }]);
    vi.useRealTimers();
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

  it("focuses the terminal textarea on touchstart with preventScroll", async () => {
    const { host } = await mountTerminal();
    expect(lastTextarea).toBeDefined();
    const focusSpy = vi.spyOn(lastTextarea!, "focus");

    const start = makeTouch("touchstart", 200);
    host.dispatchEvent(start);

    // Focus must land immediately so iOS can attach native Paste — before the
    // long-press timer fires a selection.
    expect(focusSpy).toHaveBeenCalledWith({ preventScroll: true });
    expect(start.defaultPrevented).toBe(false);
  });

  it("does not preventDefault on touchstart before scroll threshold", async () => {
    const { host } = await mountTerminal();

    const start = makeTouch("touchstart", 200);
    host.dispatchEvent(start);
    const move = makeTouch("touchmove", 198); // 2px jitter, under threshold
    host.dispatchEvent(move);

    expect(start.defaultPrevented).toBe(false);
    expect(move.defaultPrevented).toBe(false);
    expect(scrollLines).not.toHaveBeenCalled();
  });

  it("anchors the hidden textarea to the host bottom for iOS keyboard placement", () => {
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(textarea\)\s*\{[^}]*bottom:\s*0/,
    );
    expect(terminalRawViewSource).toContain('input.style.bottom = "0"');
  });

  it("resets document scroll before touchBegan focuses the textarea", () => {
    expect(terminalRawViewSource).toMatch(/touchBegan:\s*\(\)\s*=>\s*\{[\s\S]*resetDocumentScroll/);
  });

  it("terminal textarea CSS does not fully clip the edit target", () => {
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(textarea\)\s*\{[^}]*clip-path:\s*none/,
    );
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(textarea\)\s*\{[^}]*-webkit-clip-path:\s*none/,
    );
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(textarea\)\s*\{[^}]*opacity:\s*0\.01/,
    );
  });

  it("terminal textarea text and caret paint are transparent", () => {
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(textarea\)\s*\{[^}]*color:\s*transparent/,
    );
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(textarea\)\s*\{[^}]*-webkit-text-fill-color:\s*transparent/,
    );
    expect(terminalRawViewSource).toMatch(
      /\.terminal-host\s+:global\(textarea\)\s*\{[^}]*caret-color:\s*transparent/,
    );
  });

  it("hardens the textarea with transparent text paint for iOS paste", async () => {
    await mountOpenTerminal();

    await waitFor(() => {
      expect(lastTextarea).toBeDefined();
      expect(lastTextarea!.style.opacity).toBe("0.01");
      expect(lastTextarea!.style.color).toBe("transparent");
      expect(lastTextarea!.style.getPropertyValue("-webkit-text-fill-color")).toBe(
        "transparent",
      );
      expect(lastTextarea!.style.caretColor).toBe("transparent");
    });
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

  it("keeps terminal scrollback draggable while fullscreen is active", async () => {
    const { getByRole, host } = await mountOpenTerminal();
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    scrollLines.mockClear();

    getByRole("button", { name: "Expand terminal" }).click();
    await tick();

    host.dispatchEvent(makeTouch("touchstart", 200));
    const move = makeTouch("touchmove", 140);
    host.dispatchEvent(move);

    expect(document.documentElement.classList.contains("terminal-expanded")).toBe(true);
    expect(linesScrolled()).toBe(3);
    expect(move.defaultPrevented).toBe(true);
  });

  it("does not snap Ghostty back to bottom after a fullscreen scrollback drag", async () => {
    vi.useFakeTimers();
    const { getByRole, host } = await mountOpenTerminal();
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());

    getByRole("button", { name: "Expand terminal" }).click();
    await tick();
    scrollToBottom.mockClear();
    scrollLines.mockClear();

    host.dispatchEvent(makeTouch("touchstart", 200));
    const move = makeTouch("touchmove", 140);
    host.dispatchEvent(move);

    expect(linesScrolled()).toBe(3);
    vi.advanceTimersByTime(300);
    await tick();

    expect(move.defaultPrevented).toBe(true);
    expect(scrollToBottom).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("lays the mobile terminal panel flush to the top edge without a gutter margin", () => {
    const mobileBlock = terminalRawViewSource.match(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n  \}/,
    );
    expect(mobileBlock).not.toBeNull();
    const mobileCss = mobileBlock![1];
    expect(mobileCss).toMatch(/\.terminal-panel\s*\{[^}]*margin-top:\s*0/);
  });

  it("uses compact terminal chrome on mobile and desktop", () => {
    // The mobile block covers portrait width AND landscape phones (coarse
    // pointer, short viewport).
    const mobileBlock = terminalRawViewSource.match(
      /@media \(max-width: 767px\), \(pointer: coarse\) and \(max-height: 500px\) \{([\s\S]*?)\n  \}/,
    );
    expect(mobileBlock).not.toBeNull();
    const mobileCss = mobileBlock![1];

    // Mobile inherits base .terminal-host layout; no duplicate stub block.
    expect(mobileCss).not.toMatch(/\.terminal-host\s*\{[^}]*\}/);
    // Full-bleed on mobile: the panel meets the screen edges, so side/bottom
    // borders and radii go.
    expect(mobileCss).toMatch(/\.terminal-panel\s*\{[^}]*border-radius:\s*0/);
    expect(mobileCss).toMatch(/\.terminal-panel\s*\{[^}]*border-left:\s*none/);
    expect(mobileCss).not.toMatch(/\.terminal-host\s*\{[^}]*padding:\s*4px/);
    expect(mobileCss).toMatch(/\.terminal-keys\s*\{[^}]*gap:\s*4px/);
    expect(mobileCss).toMatch(/\.terminal-keys\s*\{[^}]*padding:\s*2px 4px/);
    expect(mobileCss).toMatch(/\.terminal-key\s*\{[^}]*min-height:\s*28px/);
    expect(mobileCss).toMatch(/\.terminal-key\s*\{[^}]*padding:\s*1px 7px/);
    expect(mobileCss).toMatch(/\.terminal-key\s*\{[^}]*font-size:\s*11px/);

    expect(terminalRawViewSource).not.toMatch(/\.terminal-host\s*\{[^}]*padding:\s*8px/);
    expect(terminalRawViewSource).toMatch(/\.terminal-key\s*\{[^}]*min-height:\s*28px/);
    expect(terminalRawViewSource).toMatch(/@media \(min-width: 768px\)[\s\S]*height:\s*min\(58vh,\s*560px\)/);
  });

  it("does not keep xterm-specific DOM scrollbar styling", () => {
    expect(terminalRawViewSource).not.toContain(".xterm-scrollable-element");
  });

  it("hides the terminal-keys overflow scrollbar (overlay-only, no chrome)", () => {
    expect(terminalRawViewSource).toMatch(
      /\.terminal-keys\s*\{[^}]*scrollbar-width:\s*none/,
    );
    expect(terminalRawViewSource).toMatch(
      /\.terminal-keys::-webkit-scrollbar\s*\{[^}]*display:\s*none/,
    );
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
