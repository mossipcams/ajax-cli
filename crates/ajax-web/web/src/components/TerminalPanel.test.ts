import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import TerminalPanel from "./TerminalPanel.svelte";

const write = vi.fn();
const scrollToBottom = vi.fn();
const scrollLines = vi.fn();
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
let terminalOptions: unknown;
let onScrollHandler: ((viewportY: number) => void) | undefined;
let bufferActive = { viewportY: 0, baseY: 0 };

vi.mock("@xterm/xterm", () => ({
  Terminal: class MockTerminal {
    cols = 80;
    rows = 24;
    textarea = document.createElement("textarea");
    element = document.createElement("div");
    buffer = { active: bufferActive };
    loadAddon = vi.fn();
    open = vi.fn();
    write = write;
    scrollToBottom = scrollToBottom;
    scrollLines = scrollLines;
    dispose = dispose;
    focus = focus;
    onData = vi.fn((handler: (data: string) => void) => {
      onDataHandler = handler;
    });
    onScroll = vi.fn((handler: (viewportY: number) => void) => {
      onScrollHandler = handler;
    });
    constructor(options: unknown) {
      terminalOptions = options;
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
  terminalOptions = undefined;
  onScrollHandler = undefined;
  bufferActive.viewportY = 0;
  bufferActive.baseY = 0;
  scrollLines.mockClear();
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

  it("decodes Blob websocket messages before writing to the terminal", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    const payload = JSON.stringify({ type: "output", data: btoa("blob ready") });

    socket?.emit("message", {
      data: new Blob([payload], { type: "application/json" }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("blob ready");
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

  it("does not yank the view back down while the user has scrolled up", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    // Let the open-triggered post-layout refits (which unconditionally
    // scroll to bottom) settle before simulating the user scrolling up.
    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );

    // Simulate the user scrolling away from the bottom of the scrollback.
    bufferActive.baseY = 10;
    bufferActive.viewportY = 3;
    onScrollHandler?.(3);

    scrollToBottom.mockClear();
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("status bar redraw") }),
    } as MessageEvent);

    await waitFor(() => expect(write).toHaveBeenCalledWith("status bar redraw"));
    expect(scrollToBottom).not.toHaveBeenCalled();

    // Once the user scrolls back to the bottom, auto-follow resumes.
    bufferActive.viewportY = 10;
    onScrollHandler?.(10);
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("more output") }),
    } as MessageEvent);

    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
  });

  it("shows a New output control while the user is scrolled away from bottom", async () => {
    const { getByRole, queryByRole } = render(TerminalPanel, {
      props: { handle: "web/fix-login" },
    });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );

    bufferActive.baseY = 10;
    bufferActive.viewportY = 3;
    onScrollHandler?.(3);

    scrollToBottom.mockClear();
    socket?.emit("message", {
      data: JSON.stringify({ type: "output", data: btoa("background update") }),
    } as MessageEvent);

    await waitFor(() => {
      expect(write).toHaveBeenCalledWith("background update");
      expect(getByRole("button", { name: "New output ↓" })).toBeInTheDocument();
    });
    expect(scrollToBottom).not.toHaveBeenCalled();

    getByRole("button", { name: "New output ↓" }).click();

    expect(scrollToBottom).toHaveBeenCalled();
    expect(focus).toHaveBeenCalled();
    await waitFor(() => {
      expect(queryByRole("button", { name: "New output ↓" })).not.toBeInTheDocument();
    });
  });

  it("refits immediately but debounces server resize when the visual viewport changes", async () => {
    vi.useFakeTimers();
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");
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

  it("does not scroll to bottom on viewport resize while the user is scrolled up", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    socket?.emit("open");

    await waitFor(() => expect(scrollToBottom).toHaveBeenCalled());
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );

    bufferActive.baseY = 10;
    bufferActive.viewportY = 3;
    onScrollHandler?.(3);
    scrollToBottom.mockClear();

    dispatchVisualViewport("resize");
    await waitFor(() => expect(fit).toHaveBeenCalled());

    expect(scrollToBottom).not.toHaveBeenCalled();
  });

  it("runs a second post-layout resize after the socket opens", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const socket = MockWebSocket.instances[0];
    fit.mockClear();
    socket!.send.mockClear();

    socket?.emit("open");

    await waitFor(() => {
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

  it("uses a readable mobile terminal font size", async () => {
    render(TerminalPanel, { props: { handle: "web/fix-login" } });

    await waitFor(() => {
      expect(terminalOptions).toMatchObject({ fontSize: 10 });
    });
  });

  function makeTouch(type: string, clientY: number): Event {
    const event = new Event(type, { bubbles: true, cancelable: true });
    Object.defineProperty(event, "touches", {
      value: [{ clientX: 10, clientY }],
    });
    return event;
  }

  it("scrolls local terminal scrollback on touch drag", async () => {
    const { container } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const host = container.querySelector(".task-terminal-viewport") as HTMLElement;

    // Drag the finger up ~60px. With no rendered viewport the cell height falls
    // back to 18px, so that is 3 wheel notches toward the newest output.
    host.dispatchEvent(makeTouch("touchstart", 200));
    const move = makeTouch("touchmove", 140);
    host.dispatchEvent(move);

    expect(scrollLines).toHaveBeenCalledWith(1);
    expect(scrollLines).toHaveBeenCalledTimes(3);
    // A moved touch is a scroll, not a tap: default is prevented so iOS does
    // not synthesize the click that would pop the keyboard.
    expect(move.defaultPrevented).toBe(true);
  });

  it("scrolls back into history when the finger drags downward", async () => {
    const { container } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const host = container.querySelector(".task-terminal-viewport") as HTMLElement;

    host.dispatchEvent(makeTouch("touchstart", 100));
    host.dispatchEvent(makeTouch("touchmove", 160));

    expect(scrollLines).toHaveBeenCalledWith(-1);
    expect(scrollLines.mock.calls.length).toBeGreaterThan(0);
  });

  it("leaves a stationary tap untouched so it can focus and open the keyboard", async () => {
    const { container } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    const host = container.querySelector(".task-terminal-viewport") as HTMLElement;

    host.dispatchEvent(makeTouch("touchstart", 200));
    const move = makeTouch("touchmove", 198); // 2px jitter, below the threshold
    host.dispatchEvent(move);

    expect(scrollLines).not.toHaveBeenCalled();
    expect(move.defaultPrevented).toBe(false);
  });
});
