import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import WtermTerminalView from "./WtermTerminalView.svelte";

const termWrite = vi.fn();
const termFocus = vi.fn();
const termDestroy = vi.fn();
let termOnData: ((data: string) => void) | undefined;
let termOnResize: ((cols: number, rows: number) => void) | undefined;

const ghosttyLoad = vi.hoisted(() =>
  vi.fn(() => Promise.resolve({ runtime: "ghostty-core" })),
);

vi.mock("@wterm/ghostty", () => ({
  GhosttyCore: {
    load: ghosttyLoad,
  },
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
});

afterEach(() => vi.restoreAllMocks());

describe("WtermTerminalView", () => {
  it('exposes data-terminal-engine="wterm"', async () => {
    const { getByTestId } = render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => {
      expect(getByTestId("task-terminal-panel").getAttribute("data-terminal-engine")).toBe("wterm");
    });
  });

  it("loads GhosttyCore with the distinct wterm WASM path", async () => {
    render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() =>
      expect(ghosttyLoad).toHaveBeenCalledWith({
        wasmPath: "/wterm-ghostty-vt.wasm",
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

  it("routes resize through connection.sendResize with the resize protocol", async () => {
    render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(termOnResize).toBeTypeOf("function"));
    termOnResize!(40, 20);
    expect(sendResize).toHaveBeenCalledWith(80, 20);
  });

  it("unmount calls connection.dispose and term.destroy", async () => {
    const { unmount } = render(WtermTerminalView, { props: { handle: "web/fix" } });
    await waitFor(() => expect(ghosttyLoad).toHaveBeenCalled());
    unmount();
    expect(dispose).toHaveBeenCalled();
    expect(termDestroy).toHaveBeenCalled();
  });

  it("init failure invokes onInitFailure and does not leave a live connection", async () => {
    ghosttyLoad.mockRejectedValueOnce(new Error("wasm missing"));
    const onInitFailure = vi.fn();
    render(WtermTerminalView, { props: { handle: "web/fix", onInitFailure } });
    await waitFor(() => expect(onInitFailure).toHaveBeenCalledWith("wasm missing"));
    expect(dispose).not.toHaveBeenCalled();
    await tick();
    expect(sendInput).not.toHaveBeenCalled();
  });
});
