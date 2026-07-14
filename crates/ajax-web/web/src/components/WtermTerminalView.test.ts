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

const loadWtermGhosttyCore = vi.hoisted(() =>
  vi.fn(() => Promise.resolve({ runtime: "ghostty-core" })),
);
const smokeInitWtermGhosttyCore = vi.hoisted(() => vi.fn());

vi.mock("../terminalWtermGhosttyCore", () => ({
  loadWtermGhosttyCore,
  smokeInitWtermGhosttyCore,
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
let onOutput: ((text: string) => void) | undefined;

vi.mock("../terminalConnection", () => ({
  connectTaskTerminal: vi.fn((_handle: string, events: { onOutput: (text: string) => void }) => {
    onOutput = events.onOutput;
    return {
      isOpen: () => true,
      sendInput,
      sendResize,
      reconnectNow: vi.fn(),
      dispose,
    };
  }),
}));

beforeEach(() => {
  vi.clearAllMocks();
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
