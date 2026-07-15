import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import XtermTerminalView from "./XtermTerminalView.svelte";
import { MIN_TERMINAL_COLS } from "../terminalGeometry";

const termWrite = vi.hoisted(() => vi.fn());
const termDispose = vi.hoisted(() => vi.fn());
const termOpen = vi.hoisted(() => vi.fn());
const termReset = vi.hoisted(() => vi.fn());
const termResize = vi.hoisted(() => vi.fn());
const termFocus = vi.hoisted(() => vi.fn());
const fitAddonFit = vi.hoisted(() => vi.fn());
const fitAddonDispose = vi.hoisted(() => vi.fn());
let onDataCallback: ((data: string) => void) | undefined;
let terminalCtorShouldThrow = false;
let terminalOpenShouldThrow = false;
let terminalCtorOptions: Record<string, unknown> | undefined;
let resizeObserverCallback: (() => void) | undefined;
let mockTerminalInstance:
  | { cols: number; rows: number; options: Record<string, unknown> }
  | undefined;
let mockTermCols = 80;
let mockTermRows = 24;
let proposedDims: { cols: number; rows: number } | undefined;

vi.mock("@xterm/xterm", () => ({
  Terminal: class MockTerminal {
    cols = mockTermCols;
    rows = mockTermRows;
    options: Record<string, unknown> = {};
    constructor(options?: Record<string, unknown>) {
      terminalCtorOptions = options;
      this.options = { ...(options ?? {}) };
      this.cols = mockTermCols;
      this.rows = mockTermRows;
      mockTerminalInstance = this;
      if (terminalCtorShouldThrow) {
        throw new Error("xterm constructor failed");
      }
    }
    loadAddon = vi.fn();
    open = (...args: unknown[]) => {
      if (terminalOpenShouldThrow) {
        throw new Error("xterm open failed");
      }
      return termOpen(...args);
    };
    write = termWrite;
    dispose = termDispose;
    reset = termReset;
    resize = (c: number, r: number) => {
      this.cols = c;
      this.rows = r;
      termResize(c, r);
    };
    focus = termFocus;
    onData = (cb: (data: string) => void) => {
      onDataCallback = cb;
      return { dispose: vi.fn() };
    };
  },
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class MockFitAddon {
    fit = fitAddonFit;
    dispose = fitAddonDispose;
    proposeDimensions = () => proposedDims;
  },
}));

const sendInput = vi.hoisted(() => vi.fn());
const sendResize = vi.hoisted(() => vi.fn());
const connectionDispose = vi.hoisted(() => vi.fn());
const connectTaskTerminalMock = vi.hoisted(() =>
  vi.fn((_handle: string, events: typeof connectionEvents) => {
    connectionEvents = events;
    events.onStatus?.("connected");
    return {
      isOpen: () => true,
      sendInput,
      sendResize,
      reconnectNow: vi.fn(),
      dispose: connectionDispose,
    };
  }),
);
let connectionEvents: {
  onOutput?: (text: string) => void;
  onStatus?: (status: string) => void;
  onOpen?: (isReconnect: boolean, seeded: boolean) => void;
} = {};

vi.mock("../terminalConnection", () => ({
  connectTaskTerminal: connectTaskTerminalMock,
}));

beforeEach(() => {
  termWrite.mockClear();
  termDispose.mockClear();
  termOpen.mockClear();
  termReset.mockClear();
  termResize.mockClear();
  termFocus.mockClear();
  fitAddonFit.mockClear();
  fitAddonDispose.mockClear();
  sendInput.mockClear();
  sendResize.mockClear();
  connectionDispose.mockClear();
  connectTaskTerminalMock.mockClear();
  onDataCallback = undefined;
  connectionEvents = {};
  terminalCtorShouldThrow = false;
  terminalOpenShouldThrow = false;

  resizeObserverCallback = undefined;
  terminalCtorOptions = undefined;
  mockTerminalInstance = undefined;
  mockTermCols = 80;
  mockTermRows = 24;
  proposedDims = undefined;
  vi.stubGlobal(
    "ResizeObserver",
    class MockResizeObserver {
      constructor(cb: () => void) {
        resizeObserverCallback = cb;
      }
      observe = vi.fn();
      disconnect = vi.fn();
    },
  );
  vi.stubGlobal("WebSocket", class {
    readyState = 1;
    close() {}
    addEventListener() {}
    send() {}
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("XtermTerminalView", () => {
  it("marks the panel with data-terminal-engine=xterm", async () => {
    const { getByTestId } = render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => {
      expect(getByTestId("task-terminal-panel")).toHaveAttribute("data-terminal-engine", "xterm");
    });
  });

  it("writes PTY output to the terminal", async () => {
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(connectionEvents.onOutput).toBeDefined());
    connectionEvents.onOutput?.("hello");
    expect(termWrite).toHaveBeenCalledWith("hello");
  });

  it("forwards onData to sendInput", async () => {
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(onDataCallback).toBeDefined());
    onDataCallback?.("ls\r");
    expect(sendInput).toHaveBeenCalledWith("ls\r");
  });

  it("constructs Terminal with Ghostty-matching theme", async () => {
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(terminalCtorOptions).toBeDefined());
    expect(terminalCtorOptions?.theme).toEqual(
      expect.objectContaining({
        background: "#1c1714",
        foreground: "#f4eee0",
      }),
    );
  });

  it("does not spam sendResize when ResizeObserver fires twice with unchanged cols/rows", async () => {
    vi.useFakeTimers();
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(resizeObserverCallback).toBeDefined());
    await waitFor(() => expect(sendResize).toHaveBeenCalled());

    mockTerminalInstance!.cols = 100;
    sendResize.mockClear();

    resizeObserverCallback?.();
    resizeObserverCallback?.();
    vi.advanceTimersByTime(50);

    expect(sendResize).toHaveBeenCalledTimes(1);
    expect(sendResize).toHaveBeenCalledWith(
      Math.max(100, MIN_TERMINAL_COLS),
      24,
    );
    vi.useRealTimers();
  });

  it("reports fit resize with cols at least MIN_TERMINAL_COLS", async () => {
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(fitAddonFit).toHaveBeenCalled());
    expect(sendResize).toHaveBeenCalled();
    const [cols] = sendResize.mock.calls.at(-1) ?? [];
    expect(cols).toBeGreaterThanOrEqual(MIN_TERMINAL_COLS);
  });

  it("disposes connection and terminal on unmount", async () => {
    const { unmount } = render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(termOpen).toHaveBeenCalled());
    unmount();
    expect(connectionDispose).toHaveBeenCalled();
    expect(termDispose).toHaveBeenCalled();
  });

  it("calls onInitFailure when terminal construction fails without connecting", async () => {
    const onInitFailure = vi.fn();
    terminalCtorShouldThrow = true;
    render(XtermTerminalView, { props: { handle: "web/fix", onInitFailure } });
    await waitFor(() => {
      expect(onInitFailure).toHaveBeenCalledWith("xterm constructor failed");
    });
    expect(connectTaskTerminalMock).not.toHaveBeenCalled();
  });

  it("calls onInitFailure when terminal open fails without connecting", async () => {
    const onInitFailure = vi.fn();
    terminalOpenShouldThrow = true;
    render(XtermTerminalView, { props: { handle: "web/fix", onInitFailure } });
    await waitFor(() => {
      expect(onInitFailure).toHaveBeenCalledWith("xterm open failed");
    });
    expect(connectTaskTerminalMock).not.toHaveBeenCalled();
  });

  it("floors the local grid to 80 columns with a fit font on a narrow host (parity)", async () => {
    mockTermCols = 45;
    mockTermRows = 30;
    proposedDims = { cols: 45, rows: 30 };
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(sendResize).toHaveBeenCalled());
    expect(mockTerminalInstance?.options.fontSize).toBe(Math.floor((13 * 45) / 80));
    expect(termResize).toHaveBeenCalledWith(80, expect.any(Number));
    expect(sendResize).toHaveBeenCalledWith(80, expect.any(Number));
  });

  it("keeps wide hosts unchanged (parity)", async () => {
    mockTermCols = 120;
    mockTermRows = 40;
    proposedDims = { cols: 120, rows: 40 };
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(sendResize).toHaveBeenCalled());
    expect(termResize).not.toHaveBeenCalled();
    expect(mockTerminalInstance?.options.fontSize).toBe(13);
  });

  it("resets the buffer only on a seeded reconnect (parity)", async () => {
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(connectionEvents.onOpen).toBeDefined());
    connectionEvents.onOpen?.(true, true);
    expect(termReset).toHaveBeenCalled();
    termReset.mockClear();
    connectionEvents.onOpen?.(true, false);
    expect(termReset).not.toHaveBeenCalled();
  });

  it("re-sends the PTY size after reconnect even when unchanged (parity)", async () => {
    render(XtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(connectionEvents.onOpen).toBeDefined());
    sendResize.mockClear();
    connectionEvents.onOpen?.(true, false);
    expect(sendResize).toHaveBeenCalled();
  });
});
