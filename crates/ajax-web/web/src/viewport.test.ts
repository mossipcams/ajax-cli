import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { initViewport } from "./viewport";

// Drive a fake visualViewport the way TerminalPanel.test.ts does: capture the
// handlers it registers and replay them after mutating the height.
const vvListeners: Record<string, Array<() => void>> = {};
let vvHeight = 800;

function dispatchVV(type: string) {
  for (const handler of vvListeners[type] ?? []) handler();
}

// jsdom's window persists across tests, so window/document listeners must be
// torn down or stale closures from a prior test fire on the next one.
let disposers: Array<() => void> = [];
function start(): () => void {
  const dispose = initViewport();
  disposers.push(dispose);
  return dispose;
}

beforeEach(() => {
  for (const key of Object.keys(vvListeners)) delete vvListeners[key];
  vvHeight = 800;
  disposers = [];
  document.documentElement.className = "";
  document.documentElement.removeAttribute("style");
  vi.stubGlobal("visualViewport", {
    get height() {
      return vvHeight;
    },
    addEventListener: (type: string, handler: () => void) => {
      (vvListeners[type] ??= []).push(handler);
    },
    removeEventListener: vi.fn(),
  });
  window.scrollTo = vi.fn();
});

afterEach(() => {
  for (const dispose of disposers) dispose();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("initViewport", () => {
  it("sets --app-height from visualViewport height on init", () => {
    start();
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("800px");
  });

  it("flags keyboard-open and shrinks --app-height when the viewport collapses", () => {
    start();
    vvHeight = 480; // keyboard ~320px tall
    dispatchVV("resize");
    expect(document.documentElement.classList.contains("keyboard-open")).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("480px");
  });

  it("clears keyboard-open when the viewport returns toward baseline", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    vvHeight = 800;
    dispatchVV("resize");
    expect(document.documentElement.classList.contains("keyboard-open")).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("800px");
  });

  it("snaps the document scroll back to top while the keyboard is open", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    window.dispatchEvent(new Event("scroll"));
    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
  });

  it("leaves document scroll alone when the keyboard is closed", () => {
    start();
    window.dispatchEvent(new Event("scroll"));
    expect(window.scrollTo).not.toHaveBeenCalled();
  });

  it("suppresses pinch-zoom gestures", () => {
    start();
    const event = new Event("gesturestart", { cancelable: true });
    const prevent = vi.spyOn(event, "preventDefault");
    document.dispatchEvent(event);
    expect(prevent).toHaveBeenCalled();
  });

  it("removes the keyboard-open class and --app-height on cleanup", () => {
    const dispose = initViewport();
    vvHeight = 480;
    dispatchVV("resize");
    dispose();
    expect(document.documentElement.classList.contains("keyboard-open")).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
  });

  it("no-ops without visualViewport", () => {
    vi.stubGlobal("visualViewport", undefined);
    expect(() => initViewport()()).not.toThrow();
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
  });
});
