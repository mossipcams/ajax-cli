import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Terminal } from "@xterm/xterm";
import { createTerminalScrollSync } from "./terminalScrollSync";

type FakeTerminalOpts = {
  rows?: number;
  bufferLength?: number;
  viewportY?: number;
};

function createFakeTerminal(opts: FakeTerminalOpts = {}) {
  const rows = opts.rows ?? 24;
  let bufferLength = opts.bufferLength ?? rows;
  let viewportY = opts.viewportY ?? 0;

  const scrollToBottom = vi.fn(() => {
    viewportY = Math.max(0, bufferLength - rows);
  });
  const scrollToLine = vi.fn((line: number) => {
    viewportY = line;
  });

  const term = {
    rows,
    cols: 80,
    buffer: {
      active: {
        get length() {
          return bufferLength;
        },
        get viewportY() {
          return viewportY;
        },
        getLine: () => ({ translateToString: () => "" }),
      },
    },
    scrollToBottom,
    scrollToLine,
    setBufferLength(n: number) {
      bufferLength = n;
    },
    getViewportY() {
      return viewportY;
    },
  };

  return { term: term as unknown as Terminal, scrollToBottom, scrollToLine };
}

function createInteractionEl(height = 480, scrollHeight = 480) {
  const el = document.createElement("div");
  Object.defineProperty(el, "clientHeight", { value: height, configurable: true });
  Object.defineProperty(el, "scrollHeight", { value: scrollHeight, configurable: true, writable: true });
  el.scrollTop = 0;
  return el;
}

function createScrollSync(opts: {
  term?: Terminal;
  interactionEl?: HTMLElement;
  spacerEl?: HTMLElement | null;
  onUnseenOutput?: (unseen: boolean) => void;
}) {
  const interactionEl = opts.interactionEl ?? createInteractionEl();
  const spacerEl = opts.spacerEl === undefined ? document.createElement("div") : opts.spacerEl;
  const onUnseenOutput = opts.onUnseenOutput ?? vi.fn();
  let terminal = opts.term;

  const scrollSync = createTerminalScrollSync({
    interactionEl,
    spacerEl,
    getTerminal: () => terminal,
    onUnseenOutput,
  });

  return {
    scrollSync,
    interactionEl,
    spacerEl,
    onUnseenOutput,
    setTerminal(t: Terminal | undefined) {
      terminal = t;
    },
  };
}

beforeEach(() => {
  vi.restoreAllMocks();
});

describe("createTerminalScrollSync", () => {
  it("syncSpacer sets spacer height to scrollbackLines * cellHeightPx", () => {
    const { term } = createFakeTerminal({ rows: 24, bufferLength: 124 });
    const interactionEl = createInteractionEl(480);
    const spacerEl = document.createElement("div");
    const { scrollSync } = createScrollSync({ term, interactionEl, spacerEl });

    scrollSync.syncSpacer();

    const cellHeight = Math.max(1, 480 / 24);
    const scrollback = 124 - 24;
    expect(spacerEl.style.height).toBe(`${scrollback * cellHeight}px`);
  });

  it("syncSpacer is a no-op when spacerEl is null or the terminal is absent", () => {
    const { term } = createFakeTerminal({ rows: 24, bufferLength: 100 });
    const interactionEl = createInteractionEl(480);
    const spacerWithTerminal = document.createElement("div");
    const withoutSpacer = createScrollSync({ term, interactionEl, spacerEl: null });
    withoutSpacer.scrollSync.syncSpacer();
    expect(withoutSpacer.spacerEl).toBeNull();

    const withoutTerminal = createScrollSync({ interactionEl, spacerEl: spacerWithTerminal });
    withoutTerminal.setTerminal(undefined);
    withoutTerminal.scrollSync.syncSpacer();
    expect(spacerWithTerminal.style.height).toBe("");
  });

  it("refreshFollow sets follow true at bottom, false when scrolled up, and clears unseen only at bottom", () => {
    const interactionEl = createInteractionEl(200, 1000);
    const onUnseenOutput = vi.fn();
    const { scrollSync } = createScrollSync({ interactionEl, onUnseenOutput });

    interactionEl.scrollTop = 400;
    scrollSync.refreshFollow();
    expect(onUnseenOutput).not.toHaveBeenCalled();

    // followLive is private, so observe it through its only consequence:
    // applyOutput signals unseen output instead of scrolling when not following.
    // Without this, refreshFollow could set follow unconditionally true and
    // nothing here would fail — onUnseenOutput is driven by atBottom, not by
    // followLive.
    scrollSync.applyOutput();
    expect(onUnseenOutput).toHaveBeenCalledWith(true);
    onUnseenOutput.mockClear();

    interactionEl.scrollTop = 800;
    scrollSync.refreshFollow();
    expect(onUnseenOutput).toHaveBeenCalledWith(false);
  });

  it("applyOutput while following scrolls terminal and wrapper to bottom", () => {
    const { term, scrollToBottom } = createFakeTerminal({ rows: 24, bufferLength: 80 });
    const interactionEl = createInteractionEl(200, 1000);
    interactionEl.scrollTop = 0;
    const onUnseenOutput = vi.fn();
    const { scrollSync } = createScrollSync({ term, interactionEl, onUnseenOutput });

    scrollSync.applyOutput();

    expect(scrollToBottom).toHaveBeenCalled();
    expect(interactionEl.scrollTop).toBe(800);
    expect(onUnseenOutput).not.toHaveBeenCalledWith(true);
  });

  it("applyOutput while not following signals unseen output and does not scroll", () => {
    const { term, scrollToBottom } = createFakeTerminal({ rows: 24, bufferLength: 80 });
    const interactionEl = createInteractionEl(200, 1000);
    interactionEl.scrollTop = 0;
    const onUnseenOutput = vi.fn();
    const { scrollSync } = createScrollSync({ term, interactionEl, onUnseenOutput });

    scrollSync.setFollowLive(false);
    const scrollTopBefore = interactionEl.scrollTop;
    scrollSync.applyOutput();

    expect(scrollToBottom).not.toHaveBeenCalled();
    expect(interactionEl.scrollTop).toBe(scrollTopBefore);
    expect(onUnseenOutput).toHaveBeenCalledWith(true);
  });

  it("onInteractionScroll is suppressed while setSyncingScroll(true)", () => {
    const { term, scrollToLine } = createFakeTerminal({ rows: 24, bufferLength: 80 });
    const interactionEl = createInteractionEl(200, 1000);
    interactionEl.scrollTop = 400;
    const { scrollSync } = createScrollSync({ term, interactionEl });

    scrollSync.setSyncingScroll(true);
    scrollSync.onInteractionScroll();
    expect(scrollToLine).not.toHaveBeenCalled();

    scrollSync.setSyncingScroll(false);
    scrollSync.onInteractionScroll();
    expect(scrollToLine).toHaveBeenCalled();
  });

  it("onTermScroll is suppressed while syncing or while the wrapper drove the scroll", () => {
    const { term, scrollToLine } = createFakeTerminal({ rows: 24, bufferLength: 80 });
    const interactionEl = createInteractionEl(200, 1000);
    interactionEl.scrollTop = 400;
    const { scrollSync } = createScrollSync({ term, interactionEl });

    scrollSync.setSyncingScroll(true);
    const scrollTopWhileSyncing = interactionEl.scrollTop;
    scrollSync.onTermScroll();
    expect(interactionEl.scrollTop).toBe(scrollTopWhileSyncing);

    scrollSync.setSyncingScroll(false);
    scrollToLine.mockImplementation(() => {
      scrollSync.onTermScroll();
    });
    scrollSync.onInteractionScroll();
    expect(scrollToLine).toHaveBeenCalled();
    expect(interactionEl.scrollTop).toBe(400);
  });
});
