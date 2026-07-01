import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, waitFor, queryByRole } from "@testing-library/svelte";
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
  it("defaults_to_raw_terminal_on_mobile_and_opens_the_socket", async () => {
    stubMatchMedia(true);

    const { container } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    await tick();

    await waitFor(() => expect(openTaskTerminalSocket).toHaveBeenCalledWith("web/fix-login"));
    expect(fetchTaskSnapshot).not.toHaveBeenCalled();
    expect(queryByRole(container, "tablist", { name: "Terminal mode" })).not.toBeInTheDocument();
  });

  it("defaults to the raw terminal on desktop and opens the socket", async () => {
    stubMatchMedia(false);

    const { container } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    await tick();

    await waitFor(() => expect(openTaskTerminalSocket).toHaveBeenCalledWith("web/fix-login"));
    expect(queryByRole(container, "tablist", { name: "Terminal mode" })).not.toBeInTheDocument();
  });

  it("does not render snapshot viewer or mode tabs on any viewport", async () => {
    stubMatchMedia(true);

    const { container } = render(TerminalPanel, { props: { handle: "web/fix-login" } });
    await tick();

    expect(queryByRole(container, "tab", { name: "Live" })).not.toBeInTheDocument();
    expect(queryByRole(container, "tab", { name: "Raw terminal" })).not.toBeInTheDocument();
    expect(container.querySelector(".terminal-snapshot-lines")).not.toBeInTheDocument();
    expect(localStorage.getItem("ajax.terminal.mode")).toBeNull();
  });
});
