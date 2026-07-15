import { describe, it, expect, vi } from "vitest";
import { attachTerminalGestures } from "./terminalGestures";

describe("attachTerminalGestures native scroll", () => {
  function makeTouch(type: string, clientY: number, clientX = 10): TouchEvent {
    const event = new Event(type, { bubbles: true, cancelable: true }) as TouchEvent;
    Object.defineProperty(event, "touches", {
      value: [{ clientX, clientY }],
    });
    return event;
  }

  it("leaves vertical drags uncancelled so native scrolling can run (native scroll)", () => {
    const host = document.createElement("div");
    attachTerminalGestures(host, {
      fontSize: () => 13,
      maxFontSize: () => 20,
      setFontSize: vi.fn(),
    });

    host.dispatchEvent(makeTouch("touchstart", 200, 10));
    const moveEvent = makeTouch("touchmove", 140, 10);
    host.dispatchEvent(moveEvent);

    expect(moveEvent.defaultPrevented).toBe(false);
  });
});
