import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { initViewport, isKeyboardOpen } from "./viewport";

// Drive a fake visualViewport the way TerminalPanel.test.ts does: capture the
// handlers it registers and replay them after mutating the height.
const vvListeners: Record<string, Array<() => void>> = {};
let vvHeight = 800;
let vvOffsetTop = 0;

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
  vvOffsetTop = 0;
  disposers = [];
  document.documentElement.className = "";
  document.documentElement.removeAttribute("style");
  vi.stubGlobal("visualViewport", {
    get height() {
      return vvHeight;
    },
    get offsetTop() {
      return vvOffsetTop;
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

  it("sets --app-top from visualViewport offsetTop on init", () => {
    vvOffsetTop = 44;
    start();
    expect(document.documentElement.style.getPropertyValue("--app-top")).toBe("44px");
  });

  it("updates --app-top when the visual viewport scrolls", () => {
    start();
    vvOffsetTop = 72;
    dispatchVV("scroll");
    expect(document.documentElement.style.getPropertyValue("--app-top")).toBe("72px");
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

  it("does not snap document scroll while the keyboard is open", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    window.dispatchEvent(new Event("scroll"));
    expect(window.scrollTo).not.toHaveBeenCalled();
  });

  it("leaves document scroll alone when the keyboard is closed", () => {
    start();
    window.dispatchEvent(new Event("scroll"));
    expect(window.scrollTo).not.toHaveBeenCalled();
  });

  it("clears document scroll when the keyboard closes", () => {
    start();
    const scrollCallsBeforeOpen = (window.scrollTo as ReturnType<typeof vi.fn>).mock.calls.length;

    vvHeight = 600;
    dispatchVV("resize");
    expect((window.scrollTo as ReturnType<typeof vi.fn>).mock.calls.length).toBe(
      scrollCallsBeforeOpen,
    );

    vvHeight = 800;
    dispatchVV("resize");
    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
  });

  it("suppresses pinch-zoom gestures", () => {
    start();
    const event = new Event("gesturestart", { cancelable: true });
    const prevent = vi.spyOn(event, "preventDefault");
    document.dispatchEvent(event);
    expect(prevent).toHaveBeenCalled();
  });

  it("prevents pinch touchmove page zoom", () => {
    start();
    const event = new Event("touchmove", { cancelable: true });
    Object.defineProperty(event, "scale", { value: 2 });
    document.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it("prevents two-finger touchstart page zoom", () => {
    start();
    const twoFinger = new Event("touchstart", { cancelable: true });
    Object.defineProperty(twoFinger, "touches", {
      value: [
        { clientX: 100, clientY: 100 },
        { clientX: 200, clientY: 100 },
      ],
    });
    document.dispatchEvent(twoFinger);
    expect(twoFinger.defaultPrevented).toBe(true);

    const oneFinger = new Event("touchstart", { cancelable: true });
    Object.defineProperty(oneFinger, "touches", {
      value: [{ clientX: 100, clientY: 100 }],
    });
    document.dispatchEvent(oneFinger);
    expect(oneFinger.defaultPrevented).toBe(false);
  });

  it("stops preventing two-finger touchstart after cleanup", () => {
    const dispose = start();
    dispose();
    const event = new Event("touchstart", { cancelable: true });
    Object.defineProperty(event, "touches", {
      value: [
        { clientX: 100, clientY: 100 },
        { clientX: 200, clientY: 100 },
      ],
    });
    document.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });

  it("leaves single-finger touchmove alone", () => {
    const dispose = start();
    const noScale = new Event("touchmove", { cancelable: true });
    document.dispatchEvent(noScale);
    expect(noScale.defaultPrevented).toBe(false);

    const scaleOne = new Event("touchmove", { cancelable: true });
    Object.defineProperty(scaleOne, "scale", { value: 1 });
    document.dispatchEvent(scaleOne);
    expect(scaleOne.defaultPrevented).toBe(false);

    dispose();
    const afterCleanup = new Event("touchmove", { cancelable: true });
    Object.defineProperty(afterCleanup, "scale", { value: 2 });
    document.dispatchEvent(afterCleanup);
    expect(afterCleanup.defaultPrevented).toBe(false);
  });

  it("removes the keyboard-open class, --app-height, and --app-top on cleanup", () => {
    const dispose = initViewport();
    vvOffsetTop = 36;
    vvHeight = 480;
    dispatchVV("resize");
    dispose();
    expect(document.documentElement.classList.contains("keyboard-open")).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
    expect(document.documentElement.style.getPropertyValue("--app-top")).toBe("");
  });

  it("no-ops without visualViewport", () => {
    vi.stubGlobal("visualViewport", undefined);
    expect(() => initViewport()()).not.toThrow();
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
  });
});

describe("isKeyboardOpen", () => {
  // The one keyboard truth: consumers (the terminal's PTY-lockstep freeze)
  // must agree with the CSS takeover, which keys off the same class.
  it("reflects the keyboard-open class initViewport maintains", () => {
    start();
    expect(isKeyboardOpen()).toBe(false);

    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    vvHeight = 800;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(false);
  });

  it("applies close hysteresis so address-bar drift cannot flap the state", () => {
    start();
    vvHeight = 480; // 320px delta: clearly a keyboard
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    // Partial recovery (delta 120px) sits between the 100px close and 150px
    // open thresholds: the keyboard must stay open, not flap.
    vvHeight = 680;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    vvHeight = 790; // delta 10px: settled closed
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(false);
  });

  it("rebases the baseline after closed-state drift so the next open is detected", () => {
    start();
    // Address-bar collapse shrinks the viewport 60px without a keyboard.
    vvHeight = 740;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(false);

    // A real keyboard measured from the drifted baseline (740 - 560 = 180px).
    vvHeight = 560;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);
  });

  it("rebases instead of opening the keyboard when the viewport width changes", () => {
    vi.stubGlobal("innerWidth", 390);
    start();

    vi.stubGlobal("innerWidth", 844);
    vvHeight = 390;
    dispatchVV("resize");

    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("390px");

    vvHeight = 200;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);
  });
});
