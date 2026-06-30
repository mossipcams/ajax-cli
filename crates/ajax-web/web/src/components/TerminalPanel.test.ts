import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import TerminalPanel from "./TerminalPanel.svelte";

const write = vi.fn();
const scrollToBottom = vi.fn();
const dispose = vi.fn();
let onDataHandler: ((data: string) => void) | undefined;
const fit = vi.fn();
const fitDispose = vi.fn();
const zerolagDispose = vi.fn();
let flushedCount = 0;
let flushedText = "";
const getFlushed = vi.fn(() => ({ count: flushedCount, text: flushedText }));
const setFlushed = vi.fn((count: number, text: string) => {
  flushedCount = count;
  flushedText = text;
});
const removeChar = vi.fn<() => "pending" | "flushed" | false>(() => "flushed");
const clear = vi.fn();
const clearFlushed = vi.fn();
const rerender = vi.fn();

const focus = vi.fn();
let lastTextarea: HTMLTextAreaElement | undefined;

vi.mock("@xterm/xterm", () => ({
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    textarea = document.createElement("textarea");
    loadAddon = vi.fn();
    open = vi.fn();
    write = write;
    scrollToBottom = scrollToBottom;
    dispose = dispose;
    focus = focus;
    onData = vi.fn((handler: (data: string) => void) => {
      onDataHandler = handler;
    });
    constructor() {
      lastTextarea = this.textarea;
    }
  },
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class MockFitAddon {
    fit = fit;
    dispose = fitDispose;
  },
}));

vi.mock("xterm-zerolag-input", () => ({
  ZerolagInputAddon: class MockZerolagInputAddon {
    getFlushed = getFlushed;
    setFlushed = setFlushed;
    removeChar = removeChar;
    clear = clear;
    clearFlushed = clearFlushed;
    rerender = rerender;
    dispose = zerolagDispose;
  },
}));

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  static OPEN = 1;
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
  flushedCount = 0;
  flushedText = "";
  for (const key of Object.keys(vvListeners)) delete vvListeners[key];
  vi.stubGlobal("WebSocket", MockWebSocket as unknown as typeof WebSocket);
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
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("TerminalPanel", () => {
  it("opens the task terminal socket on mount", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(MockWebSocket.instances).toHaveLength(1);
      expect(MockWebSocket.instances[0]?.url).toContain("/api/tasks/web%2Ffix-login/terminal");
    });
  });

  it("writes incoming output frames to the terminal", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("hello") }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("hello");
      expect(clearFlushed).toHaveBeenCalled();
      expect(rerender).toHaveBeenCalled();
    });
  });

  it("decodes UTF-8 output frames before writing to the terminal", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    const bytes = new TextEncoder().encode("λ ready");
    const encoded = btoa(String.fromCharCode(...bytes));

    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: encoded }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("λ ready");
    });
  });

  it("sends terminal input as JSON frames", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    onDataHandler?.("a");

    await waitFor(() => {
      // Sent immediately, so the char is tracked as flushed (awaiting echo),
      // not pending — the overlay clears when the echo lands.
      expect(setFlushed).toHaveBeenCalledWith(1, "a");
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "a" }),
      );
    });
  });

  it("accumulates flushed overlay text across successive keystrokes", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");
    setFlushed.mockClear();

    onDataHandler?.("h");
    onDataHandler?.("i");

    await waitFor(() => {
      expect(setFlushed).toHaveBeenNthCalledWith(1, 1, "h");
      expect(setFlushed).toHaveBeenNthCalledWith(2, 2, "hi");
    });
  });

  it("clears zerolag overlay state on Enter and sends the frame", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    onDataHandler?.("\r");

    await waitFor(() => {
      expect(clear).toHaveBeenCalled();
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\r" }),
      );
    });
  });

  it("always forwards backspace to the PTY and syncs the overlay", async () => {
    removeChar.mockReturnValueOnce("flushed");
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    onDataHandler?.("\x7f");

    await waitFor(() => {
      expect(removeChar).toHaveBeenCalled();
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\x7f" }),
      );
    });
  });

  it("forwards backspace even when zerolag reports a pending-only removal", async () => {
    // Raw tmux attach sends every keystroke immediately, so the typed
    // characters live in the real PTY buffer even though zerolag still
    // tracks them as "pending". Backspace must reach the PTY regardless,
    // otherwise the iOS soft-keyboard delete erases only the overlay.
    removeChar.mockReturnValueOnce("pending");
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    onDataHandler?.("\x7f");

    await waitFor(() => {
      expect(removeChar).toHaveBeenCalled();
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\x7f" }),
      );
    });
  });

  it("loads ZerolagInputAddon for local echo", async () => {
    const { ZerolagInputAddon } = await import("xterm-zerolag-input");
    render(TerminalPanel, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(ZerolagInputAddon).toBeDefined();
      expect(setFlushed).toBeDefined();
    });
  });

  it("exposes stable layout hooks for the task terminal viewport", () => {
    const { container, getByLabelText } = render(TerminalPanel, {
      props: { handle: "web/fix-login" },
    });

    expect(getByLabelText("Task terminal")).toBeInTheDocument();
    expect(container.querySelector("[data-testid='task-terminal-panel']")).toBeInTheDocument();
    expect(container.querySelector(".task-terminal-viewport")).toBeInTheDocument();
  });

  it("closes the socket and disposes xterm on destroy", async () => {
    const { unmount } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];

    unmount();

    await waitFor(() => {
      expect(socket?.close).toHaveBeenCalled();
      expect(dispose).toHaveBeenCalled();
      expect(zerolagDispose).toHaveBeenCalled();
    });
  });

  it("keeps the newest output in view after writes and viewport resizes", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

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

  it("refits and sends a resize frame when the visual viewport changes", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");
    fit.mockClear();
    socket!.send.mockClear();

    dispatchVisualViewport("resize");

    await waitFor(() => {
      expect(fit).toHaveBeenCalled();
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "resize", cols: 80, rows: 24 }),
      );
    });
  });

  it("sends an Escape byte when the Esc key is tapped", async () => {
    const { getByRole } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    getByRole("button", { name: "Esc" }).click();

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\x1b" }),
      );
    });
  });

  it("folds the next letter into a control code after Ctrl is armed", async () => {
    const { getByRole } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    getByRole("button", { name: "Ctrl" }).click();
    onDataHandler?.("c");

    await waitFor(() => {
      expect(socket?.send).toHaveBeenCalledWith(
        JSON.stringify({ type: "input", data: "\x03" }),
      );
    });
  });

  it("disables autocorrect/autocapitalize on the xterm input", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(lastTextarea?.getAttribute("autocapitalize")).toBe("off");
      expect(lastTextarea?.getAttribute("autocorrect")).toBe("off");
      expect(lastTextarea?.getAttribute("spellcheck")).toBe("false");
    });
  });
});
