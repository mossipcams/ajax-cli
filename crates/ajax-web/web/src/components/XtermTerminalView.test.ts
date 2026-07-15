import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import XtermTerminalView from "./XtermTerminalView.svelte";
import { MIN_TERMINAL_COLS } from "../terminalGeometry";

const termWrite = vi.hoisted(() => vi.fn());
const termDispose = vi.hoisted(() => vi.fn());
const termOpen = vi.hoisted(() => vi.fn());
const fitAddonFit = vi.hoisted(() => vi.fn());
const fitAddonDispose = vi.hoisted(() => vi.fn());
let onDataCallback: ((data: string) => void) | undefined;
let terminalCtorShouldThrow = false;
let terminalOpenShouldThrow = false;

vi.mock("@xterm/xterm", () => ({
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    constructor() {
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
} = {};

vi.mock("../terminalConnection", () => ({
  connectTaskTerminal: connectTaskTerminalMock,
}));

beforeEach(() => {
  termWrite.mockClear();
  termDispose.mockClear();
  termOpen.mockClear();
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

  vi.stubGlobal(
    "ResizeObserver",
    class MockResizeObserver {
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
});
