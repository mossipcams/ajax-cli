import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useMobileKeyboard } from "./useMobileKeyboard";

const vvListeners: Record<string, Array<() => void>> = {};
let vvHeight = 800;
let innerHeight = 800;
let coarsePointer = true;

function dispatchVV(type: string) {
  for (const handler of vvListeners[type] ?? []) handler();
}

beforeEach(() => {
  for (const key of Object.keys(vvListeners)) delete vvListeners[key];
  vvHeight = 800;
  innerHeight = 800;
  coarsePointer = true;
  vi.stubGlobal("innerHeight", innerHeight);
  vi.stubGlobal("visualViewport", {
    get height() {
      return vvHeight;
    },
    get offsetTop() {
      return 0;
    },
    addEventListener: (type: string, handler: () => void) => {
      (vvListeners[type] ??= []).push(handler);
    },
    removeEventListener: vi.fn(),
  });
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({
      matches: coarsePointer,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  );
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    cb(0);
    return 1;
  });
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("useMobileKeyboard", () => {
  it("reports keyboardHeight when visualViewport shrinks but innerHeight does not", async () => {
    const { result } = renderHook(() => useMobileKeyboard());
    expect(result.current.keyboardHeight).toBe(0);

    vvHeight = 500;
    act(() => dispatchVV("resize"));

    expect(result.current.keyboardOpen).toBe(true);
    expect(result.current.keyboardHeight).toBeGreaterThanOrEqual(250);
  });

  it("keeps keyboardHeight at 0 when innerHeight shrinks with visualViewport", async () => {
    const { result } = renderHook(() => useMobileKeyboard());
    innerHeight = 500;
    vvHeight = 500;
    vi.stubGlobal("innerHeight", innerHeight);
    act(() => dispatchVV("resize"));

    expect(result.current.keyboardOpen).toBe(true);
    expect(result.current.keyboardHeight).toBe(0);
  });

  it("clears keyboardHeight on blur while visualViewport stays shrunken", async () => {
    const { result } = renderHook(() => useMobileKeyboard());
    vvHeight = 500;
    act(() => dispatchVV("resize"));
    expect(result.current.keyboardOpen).toBe(true);
    expect(result.current.keyboardHeight).toBeGreaterThan(0);

    const textarea = document.createElement("textarea");
    document.body.appendChild(textarea);
    act(() => {
      textarea.focus();
      document.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    });
    act(() => {
      textarea.blur();
      document.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });

    expect(vvHeight).toBe(500);
    expect(result.current.keyboardOpen).toBe(false);
    expect(result.current.keyboardHeight).toBe(0);
  });

  it("clears keyboard latch when PWA innerHeight restores without visualViewport resize (#1106)", async () => {
    const { result } = renderHook(() => useMobileKeyboard());
    innerHeight = 520;
    vvHeight = 520;
    vi.stubGlobal("innerHeight", innerHeight);
    act(() => dispatchVV("resize"));
    expect(result.current.keyboardOpen).toBe(true);
    expect(result.current.keyboardHeight).toBe(0);

    innerHeight = 800;
    vi.stubGlobal("innerHeight", innerHeight);
    act(() => window.dispatchEvent(new Event("resize")));

    expect(vvHeight).toBe(520);
    expect(result.current.keyboardOpen).toBe(false);
    expect(result.current.keyboardHeight).toBe(0);
  });
});
