import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import TerminalPanel from "./TerminalPanel.svelte";

const openTaskTerminalSocket = vi.fn();
const fetchTaskSnapshot = vi.fn();
const sendTaskKeys = vi.fn();

vi.mock("../api", () => ({
  openTaskTerminalSocket: (handle: string) => openTaskTerminalSocket(handle),
  fetchTaskSnapshot: (handle: string, since?: string) => fetchTaskSnapshot(handle, since),
  sendTaskKeys: (handle: string, text: string, submit: boolean) =>
    sendTaskKeys(handle, text, submit),
}));

// The raw view mounts xterm; stub it so switching to raw mode is harmless here.
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    textarea = document.createElement("textarea");
    buffer = { active: { viewportY: 0, baseY: 0 } };
    loadAddon = vi.fn();
    open = vi.fn();
    write = vi.fn();
    scrollToBottom = vi.fn();
    scrollLines = vi.fn();
    dispose = vi.fn();
    focus = vi.fn();
    onData = vi.fn();
    onScroll = vi.fn();
  },
}));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit = vi.fn();
    dispose = vi.fn();
  },
}));
vi.mock("xterm-zerolag-input", () => ({
  ZerolagInputAddon: class {
    getFlushed = vi.fn(() => ({ count: 0, text: "" }));
    setFlushed = vi.fn();
    removeChar = vi.fn();
    clear = vi.fn();
    clearFlushed = vi.fn();
    rerender = vi.fn();
    dispose = vi.fn();
  },
}));

function fakeSocket() {
  return {
    readyState: 1,
    send: vi.fn(),
    close: vi.fn(),
    addEventListener: vi.fn(),
  };
}

function stubMatchMedia(mobile: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => ({
      matches: mobile && query.includes("max-width: 767px"),
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  );
}

beforeEach(() => {
  openTaskTerminalSocket.mockReset().mockImplementation(() => fakeSocket());
  fetchTaskSnapshot
    .mockReset()
    .mockResolvedValue({ sequence_changed: true, lines: [], truncated: false, sequence: "a", summary: null });
  sendTaskKeys.mockReset().mockResolvedValue({ ok: true });
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("TerminalPanel host", () => {
  it("lands in the snapshot viewer on mobile and never opens the raw socket", async () => {
    stubMatchMedia(true);

    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    await tick();

    await waitFor(() => expect(fetchTaskSnapshot).toHaveBeenCalledWith("web/fix-login", undefined));
    expect(openTaskTerminalSocket).not.toHaveBeenCalled();
  });

  it("defaults to the raw terminal on desktop and opens the socket", async () => {
    stubMatchMedia(false);

    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    await tick();

    await waitFor(() => expect(openTaskTerminalSocket).toHaveBeenCalledWith("web/fix-login"));
  });

  it("only opens the raw socket on mobile after an explicit opt-in", async () => {
    stubMatchMedia(true);

    const { getByRole } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    await tick();
    expect(openTaskTerminalSocket).not.toHaveBeenCalled();

    getByRole("tab", { name: "Raw terminal" }).click();
    await tick();

    await waitFor(() => expect(openTaskTerminalSocket).toHaveBeenCalledWith("web/fix-login"));
  });

  it("persists the chosen mode across mounts", async () => {
    stubMatchMedia(true);

    const first = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    await tick();
    first.getByRole("tab", { name: "Raw terminal" }).click();
    await tick();
    first.unmount();

    openTaskTerminalSocket.mockClear();
    render(TerminalPanel, { props: { handle: "web/fix-login" } });
    await tick();

    // The saved "raw" choice sticks even though the viewport is mobile.
    await waitFor(() => expect(openTaskTerminalSocket).toHaveBeenCalledWith("web/fix-login"));
  });
});
