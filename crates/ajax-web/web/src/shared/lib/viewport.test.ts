import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { initViewport, isKeyboardOpen, resetDocumentScroll } from "./viewport";

// Drive a fake visualViewport: capture the handlers it registers and replay
// them after mutating the height. The keyboard band pin contract that consumes
// these values is covered separately in `components/keyboardBandPin.test.ts`.
const vvListeners: Record<string, Array<() => void>> = {};
let vvHeight = 800;
let vvOffsetTop = 0;

function dispatchVV(type: string) {
  for (const handler of vvListeners[type] ?? []) handler();
}

let visibilityState: DocumentVisibilityState = "visible";

function setVisibilityState(state: DocumentVisibilityState) {
  visibilityState = state;
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    get: () => visibilityState,
  });
}

function dispatchVisibilityChange() {
  document.dispatchEvent(new Event("visibilitychange"));
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
  vi.useFakeTimers();
  for (const key of Object.keys(vvListeners)) delete vvListeners[key];
  vvHeight = 800;
  vvOffsetTop = 0;
  visibilityState = "visible";
  disposers = [];
  document.documentElement.className = "";
  document.documentElement.removeAttribute("style");
  document.documentElement.removeAttribute("data-session-viewport");
  setVisibilityState("visible");
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
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

/** Expansion must persist for the close-settle window before the class drops. */
function settleClose() {
  vi.advanceTimersByTime(400);
}

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

  it("clears document scroll when the keyboard opens", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
  });

  it("clears keyboard-open when the viewport returns toward baseline and settles", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    vvHeight = 800;
    dispatchVV("resize");
    settleClose();
    expect(document.documentElement.classList.contains("keyboard-open")).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("800px");
  });

  it("clears --app-height when session owns viewport with iOS closed-keyboard discrepancy", () => {
    vi.stubGlobal("innerHeight", 800);
    vvHeight = 766;
    document.documentElement.setAttribute("data-session-viewport", "owned");
    start();
    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
  });

  it("clears short visualViewport pin on session resize while keyboard is closed", () => {
    vi.stubGlobal("innerHeight", 800);
    document.documentElement.setAttribute("data-session-viewport", "owned");
    start();
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");

    vvHeight = 766;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
  });

  it("clears stale keyboard band geometry after keyboard close when visualViewport stays stale", () => {
    vi.stubGlobal("innerHeight", 800);
    document.documentElement.setAttribute("data-session-viewport", "owned");
    const composer = document.createElement("textarea");
    document.body.appendChild(composer);
    start();
    composer.focus();
    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    composer.blur();
    vvHeight = 800;
    dispatchVV("resize");
    settleClose();
    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");

    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");

    composer.remove();
  });

  it("restores layout height when session composer blurs with a stale visualViewport", async () => {
    vi.stubGlobal("innerHeight", 800);
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      cb(0);
      return 1;
    });
    document.documentElement.setAttribute("data-session-viewport", "owned");
    const composer = document.createElement("textarea");
    const shell = document.createElement("form");
    shell.setAttribute("data-testid", "session-composer");
    shell.appendChild(composer);
    document.body.appendChild(shell);

    start();
    composer.focus();
    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("480px");

    composer.blur();
    document.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));

    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");

    shell.remove();
    document.documentElement.removeAttribute("data-session-viewport");
  });

  // #1113: iOS PWA may restore innerHeight while vv stays short during Send/Attach;
  // stale-visualViewport dismiss must not blur mid-gesture.
  it("defers PWA stale-viewport dismiss while pointer is down inside session composer (#1113)", () => {
    vi.stubGlobal("innerHeight", 800);
    document.documentElement.setAttribute("data-session-viewport", "owned");
    const composer = document.createElement("textarea");
    const shell = document.createElement("form");
    shell.setAttribute("data-testid", "session-composer");
    shell.appendChild(composer);
    const send = document.createElement("button");
    send.type = "button";
    send.textContent = "Send";
    shell.appendChild(send);
    document.body.appendChild(shell);

    start();
    composer.focus();
    vvHeight = 520;
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      get: () => 520,
    });
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("520px");
    expect(composer).toHaveFocus();

    send.dispatchEvent(new Event("pointerdown", { bubbles: true }));

    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      get: () => 800,
    });
    window.dispatchEvent(new Event("resize"));

    expect(isKeyboardOpen()).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("520px");
    expect(composer).toHaveFocus();

    send.dispatchEvent(new Event("pointerup", { bubbles: true }));

    // Synthetic click has not landed yet; defer blur/relayout until next tick.
    expect(isKeyboardOpen()).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("520px");
    expect(composer).toHaveFocus();

    vi.advanceTimersByTime(0);

    // iOS PWA often omits a second window.resize after pointerup.
    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");

    shell.remove();
  });

  it("restores after composer pointerup when visualViewport stays stale (#1113)", () => {
    vi.stubGlobal("innerHeight", 800);
    document.documentElement.setAttribute("data-session-viewport", "owned");
    const composer = document.createElement("textarea");
    const shell = document.createElement("form");
    shell.setAttribute("data-testid", "session-composer");
    shell.appendChild(composer);
    const send = document.createElement("button");
    send.type = "button";
    shell.appendChild(send);
    document.body.appendChild(shell);

    start();
    composer.focus();
    vvHeight = 520;
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      get: () => 520,
    });
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    send.dispatchEvent(new Event("pointerdown", { bubbles: true }));
    Object.defineProperty(window, "innerHeight", {
      configurable: true,
      get: () => 800,
    });
    window.dispatchEvent(new Event("resize"));
    expect(isKeyboardOpen()).toBe(true);

    send.dispatchEvent(new Event("pointerup", { bubbles: true }));
    expect(isKeyboardOpen()).toBe(true);
    expect(composer).toHaveFocus();

    vi.advanceTimersByTime(0);
    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");

    shell.remove();
  });

  it("absorbs a transient viewport expansion while typing (no teardown)", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    // iOS momentarily reports an expanded viewport mid-typing (keyboard morph,
    // autocorrect popover). The pinned layout must not tear down for it.
    vvHeight = 800;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);
    // Geometry holds too: a band snap to full height is the same visual jump.
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("480px");

    vi.advanceTimersByTime(100);
    vvHeight = 480; // bounced back before the settle window elapsed
    dispatchVV("resize");
    settleClose();

    expect(isKeyboardOpen()).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("480px");
  });

  it("closes after the expansion persists for the settle window", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    vvHeight = 800;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);
    settleClose();
    expect(isKeyboardOpen()).toBe(false);
  });

  it("does not snap document scroll while the keyboard is open", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    (window.scrollTo as ReturnType<typeof vi.fn>).mockClear();
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
    vvHeight = 600;
    dispatchVV("resize");
    (window.scrollTo as ReturnType<typeof vi.fn>).mockClear();

    vvHeight = 800;
    dispatchVV("resize");
    settleClose();
    expect(window.scrollTo).toHaveBeenCalledWith(0, 0);
  });

  it("blurs the session composer when keyboard-open clears", () => {
    const composer = document.createElement("textarea");
    composer.setAttribute("aria-label", "Message");
    const shell = document.createElement("form");
    shell.setAttribute("data-testid", "session-composer");
    shell.appendChild(composer);
    document.body.appendChild(shell);
    composer.focus();

    start();
    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);
    expect(composer).toHaveFocus();

    vvHeight = 800;
    dispatchVV("resize");
    settleClose();

    expect(isKeyboardOpen()).toBe(false);
    expect(composer).not.toHaveFocus();
    shell.remove();
  });

  it("does not blur the task terminal when keyboard-open clears", () => {
    const termInput = document.createElement("textarea");
    termInput.className = "xterm-helper-textarea";
    document.body.appendChild(termInput);
    termInput.focus();

    start();
    vvHeight = 480;
    dispatchVV("resize");
    vvHeight = 800;
    dispatchVV("resize");
    settleClose();

    expect(termInput).toHaveFocus();
    termInput.remove();
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

  it("clears keyboard-open on visible visibilitychange when visualViewport already restored (#836)", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    vvHeight = 800;
    setVisibilityState("visible");
    dispatchVisibilityChange();

    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("800px");
  });

  it("rebases to layout height on visible visibilitychange when visualViewport stays stale-small (#836)", () => {
    vi.stubGlobal("innerHeight", 800);
    start();
    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    setVisibilityState("visible");
    dispatchVisibilityChange();

    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("800px");
    expect(document.documentElement.style.getPropertyValue("--app-top")).toBe("0px");
  });

  it("performs the same foreground resync on pageshow (#836)", () => {
    vi.stubGlobal("innerHeight", 800);
    start();
    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    window.dispatchEvent(new Event("pageshow"));

    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("800px");
  });

  it("clears keyboard-open on hidden visibilitychange without waiting for visualViewport", () => {
    start();
    vvHeight = 480;
    dispatchVV("resize");
    expect(isKeyboardOpen()).toBe(true);

    setVisibilityState("hidden");
    dispatchVisibilityChange();

    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("480px");
  });

  it("stops handling visibilitychange after cleanup", () => {
    const dispose = start();
    vvHeight = 480;
    dispatchVV("resize");
    dispose();

    document.documentElement.classList.add("keyboard-open");
    document.documentElement.style.setProperty("--app-height", "480px");
    setVisibilityState("visible");
    expect(() => dispatchVisibilityChange()).not.toThrow();
    expect(document.documentElement.classList.contains("keyboard-open")).toBe(true);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("480px");
  });

  it("init with visualViewport.height === 0 and usable innerHeight uses layout height (#850)", () => {
    vvHeight = 0;
    vi.stubGlobal("innerHeight", 812);
    start();
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("812px");
    expect(document.documentElement.style.getPropertyValue("--app-top")).toBe("0px");
  });

  it("init with both visual and layout height 0 leaves --app-height unset (#850)", () => {
    vvHeight = 0;
    vi.stubGlobal("innerHeight", 0);
    start();
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("");
    expect(document.documentElement.style.getPropertyValue("--app-top")).toBe("");
  });

  it("init at 0 then resize to real height updates --app-height without keyboard-open (#850)", () => {
    vvHeight = 0;
    vi.stubGlobal("innerHeight", 812);
    start();
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("812px");
    expect(isKeyboardOpen()).toBe(false);

    vvHeight = 812;
    dispatchVV("resize");

    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("812px");
    expect(isKeyboardOpen()).toBe(false);
  });

  it("does not flag keyboard-open when visualViewport stays 0 after layout init (#850)", () => {
    vvHeight = 0;
    vi.stubGlobal("innerHeight", 812);
    start();
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("812px");

    dispatchVV("resize");

    expect(isKeyboardOpen()).toBe(false);
    expect(document.documentElement.style.getPropertyValue("--app-height")).toBe("812px");
  });
});

describe("resetDocumentScroll", () => {
  it("resetDocumentScroll clears every known document scroll owner safely", () => {
    const scrollTo = vi.spyOn(window, "scrollTo").mockImplementation(() => {});
    document.documentElement.scrollTop = 120;
    document.body.scrollTop = 80;
    if (document.scrollingElement) {
      document.scrollingElement.scrollTop = 60;
    }

    resetDocumentScroll();

    expect(scrollTo).toHaveBeenCalledWith(0, 0);
    expect(document.documentElement.scrollTop).toBe(0);
    expect(document.body.scrollTop).toBe(0);
    expect(document.scrollingElement?.scrollTop ?? 0).toBe(0);

    scrollTo.mockImplementation(() => {
      throw new Error("Not implemented");
    });
    expect(() => resetDocumentScroll()).not.toThrow();
  });

  it("resetDocumentScroll also zeros the App route-scroll container", () => {
    const routeScroll = document.createElement("div");
    routeScroll.setAttribute("data-testid", "route-scroll");
    Object.defineProperty(routeScroll, "scrollTop", {
      configurable: true,
      writable: true,
      value: 240,
    });
    document.body.appendChild(routeScroll);

    resetDocumentScroll();

    expect(routeScroll.scrollTop).toBe(0);
    routeScroll.remove();
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
    settleClose();
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
    settleClose();
    expect(isKeyboardOpen()).toBe(true);

    vvHeight = 790; // delta 10px: settled closed
    dispatchVV("resize");
    settleClose();
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
