import { describe, it, expect } from "vitest";
import {
  COPY_NOTICE_MS,
  createTerminalClipboardUi,
  type ClipboardUiSnapshot,
} from "./terminalClipboard";

type Scheduled = { id: number; fireAt: number; fn: () => void };

const createHarness = (noticeMs = COPY_NOTICE_MS) => {
  let now = 0;
  let nextId = 1;
  const scheduled: Scheduled[] = [];
  const changes: ClipboardUiSnapshot[] = [];

  const ui = createTerminalClipboardUi({
    noticeMs,
    schedule: (fn, ms) => {
      const id = nextId++;
      scheduled.push({ id, fireAt: now + ms, fn });
      return id as ReturnType<typeof setTimeout>;
    },
    clearSchedule: (id) => {
      const index = scheduled.findIndex((entry) => entry.id === id);
      if (index >= 0) scheduled.splice(index, 1);
    },
    onChange: (snap) => changes.push({ ...snap }),
  });

  const advance = (ms: number) => {
    now += ms;
    const due = scheduled.filter((entry) => entry.fireAt <= now);
    for (const entry of due) {
      const index = scheduled.indexOf(entry);
      if (index >= 0) scheduled.splice(index, 1);
      entry.fn();
    }
  };

  return { ui, changes, advance, scheduled };
};

describe("terminalClipboard", () => {
  it("open/close paste fallback toggles pasteFallbackOpen", () => {
    const { ui } = createHarness();
    expect(ui.snapshot().pasteFallbackOpen).toBe(false);
    ui.openPasteFallback();
    expect(ui.snapshot().pasteFallbackOpen).toBe(true);
    ui.closePasteFallback();
    expect(ui.snapshot().pasteFallbackOpen).toBe(false);
  });

  it("takePasteFallbackText closes fallback and returns trimmed text", () => {
    const { ui } = createHarness();
    ui.openPasteFallback();
    expect(ui.takePasteFallbackText("  hello  ")).toBe("hello");
    expect(ui.snapshot().pasteFallbackOpen).toBe(false);
    expect(ui.takePasteFallbackText("")).toBe("");
  });

  it("presentCopySelection dismisses on empty text; non-empty opens overlay only", () => {
    const { ui } = createHarness();
    ui.presentCopySelection("selected");
    expect(ui.snapshot()).toMatchObject({
      copyOverlayOpen: true,
      copyFallbackOpen: false,
      copyOverlayText: "selected",
    });
    ui.presentCopySelection("");
    expect(ui.snapshot()).toMatchObject({
      copyOverlayOpen: false,
      copyFallbackOpen: false,
      copyOverlayText: "",
    });
  });

  it("beginCopyAttempt closes overlay and returns overlay text", () => {
    const { ui } = createHarness();
    ui.presentCopySelection("copy me");
    expect(ui.beginCopyAttempt()).toBe("copy me");
    expect(ui.snapshot().copyOverlayOpen).toBe(false);
  });

  it("noteCopySucceeded flashes Copied and clears after noticeMs", () => {
    const { ui, advance } = createHarness(2500);
    ui.presentCopySelection("x");
    ui.noteCopySucceeded();
    expect(ui.snapshot()).toMatchObject({
      notice: "Copied",
      copyOverlayText: "",
      copyFallbackOpen: false,
    });
    advance(2499);
    expect(ui.snapshot().notice).toBe("Copied");
    advance(1);
    expect(ui.snapshot().notice).toBe("");
  });

  it("noteCopyFailed opens copy fallback", () => {
    const { ui } = createHarness();
    ui.presentCopySelection("fail me");
    ui.beginCopyAttempt();
    ui.noteCopyFailed();
    expect(ui.snapshot()).toMatchObject({
      copyFallbackOpen: true,
      copyOverlayText: "fail me",
    });
  });

  it("dismissCopyUi clears overlay, fallback, and text", () => {
    const { ui } = createHarness();
    ui.presentCopySelection("keep");
    ui.noteCopyFailed();
    ui.dismissCopyUi();
    expect(ui.snapshot()).toMatchObject({
      copyOverlayOpen: false,
      copyFallbackOpen: false,
      copyOverlayText: "",
    });
  });

  it("dispose clears a pending notice timer", () => {
    const { ui, advance, scheduled } = createHarness(2500);
    ui.flashNotice("Saved");
    expect(ui.snapshot().notice).toBe("Saved");
    expect(scheduled.length).toBe(1);
    ui.dispose();
    expect(scheduled.length).toBe(0);
    advance(2500);
    expect(ui.snapshot().notice).toBe("Saved");
  });

  it("calls onChange after each mutation", () => {
    const { ui, changes } = createHarness();
    ui.openPasteFallback();
    ui.presentCopySelection("a");
    expect(changes.length).toBeGreaterThanOrEqual(2);
    expect(changes.at(-1)?.pasteFallbackOpen).toBe(true);
  });
});
