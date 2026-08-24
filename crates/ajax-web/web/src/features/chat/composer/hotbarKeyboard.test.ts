import { describe, it, expect, vi } from "vitest";
import {
  attachComposerHotbarKeyboardRetention,
  preventComposerHotbarFocusSteal,
  retainComposerKeyboardOnHotbarCapture,
} from "./hotbarKeyboard";

describe("hotbarKeyboard", () => {
  it("calls preventDefault for hotbar dead-space capture retention", () => {
    document.body.innerHTML =
      '<div data-testid="session-composer-hotbar"><span class="gap"></span></div>';
    const gap = document.querySelector(".gap")!;
    const event = new Event("touchstart", { cancelable: true, bubbles: true });
    Object.defineProperty(event, "target", { value: gap, configurable: true });
    const preventDefault = vi.spyOn(event, "preventDefault");

    retainComposerKeyboardOnHotbarCapture(event);

    expect(preventDefault).toHaveBeenCalledOnce();
  });

  it("does not call preventDefault when the hotbar target is a button", () => {
    document.body.innerHTML =
      '<div data-testid="session-composer-hotbar"><button type="button">Send</button></div>';
    const button = document.querySelector("button")!;
    const event = new Event("touchstart", { cancelable: true, bubbles: true });
    Object.defineProperty(event, "target", { value: button, configurable: true });
    const preventDefault = vi.spyOn(event, "preventDefault");

    retainComposerKeyboardOnHotbarCapture(event);

    expect(preventDefault).not.toHaveBeenCalled();
  });

  it("registers capture-phase hotbar listeners that cancel dead-space default", () => {
    document.body.innerHTML =
      '<div data-testid="session-composer-hotbar"><span class="gap"></span></div>';
    const hotbar = document.querySelector("[data-testid='session-composer-hotbar']") as HTMLElement;
    const gap = document.querySelector(".gap")!;
    const addSpy = vi.spyOn(hotbar, "addEventListener");

    const detach = attachComposerHotbarKeyboardRetention(hotbar);
    expect(addSpy).toHaveBeenCalledWith("touchstart", expect.any(Function), {
      capture: true,
      passive: false,
    });
    expect(addSpy).toHaveBeenCalledWith("pointerdown", expect.any(Function), {
      capture: true,
      passive: false,
    });

    const event = new Event("touchstart", { cancelable: true, bubbles: true });
    Object.defineProperty(event, "target", { value: gap, configurable: true });
    const preventDefault = vi.spyOn(event, "preventDefault");
    hotbar.dispatchEvent(event);
    expect(preventDefault).toHaveBeenCalledOnce();

    detach();
  });

  it("prevents focus steal on hotbar controls without blocking click handlers", () => {
    const event = { preventDefault: vi.fn() } as unknown as Parameters<
      typeof preventComposerHotbarFocusSteal
    >[0];
    preventComposerHotbarFocusSteal(event);
    expect(event.preventDefault).toHaveBeenCalledOnce();
  });
});
