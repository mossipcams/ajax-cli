import { describe, it, expect, vi } from "vitest";
import { createRef } from "react";
import {
  attachComposerHotbarKeyboardRetention,
  attachToolbarKeyboardRetention,
  blurComposerOnPointerDown,
  isComposerKeyboardDismissTarget,
  preventComposerHotbarFocusSteal,
  retainComposerKeyboardOnHotbarCapture,
  retainToolbarKeyboardOnCapture,
} from "./composerBlur";

describe("composerBlur", () => {
  it("treats the composer hotbar as keyboard-dismiss exempt", () => {
    document.body.innerHTML =
      '<div data-testid="session-composer-hotbar"><span class="gap"></span></div>';
    const gap = document.querySelector(".gap")!;
    expect(isComposerKeyboardDismissTarget(gap)).toBe(false);
  });

  it("still dismisses when tapping non-interactive chrome above the hotbar", () => {
    document.body.innerHTML = '<main data-testid="session-thread"><p>Hi</p></main>';
    const paragraph = document.querySelector("p")!;
    expect(isComposerKeyboardDismissTarget(paragraph)).toBe(true);
  });

  it("does not blur the composer when tapping hotbar dead space", () => {
    document.body.innerHTML =
      '<div data-testid="session-composer-hotbar"><span class="gap"></span></div>';
    const composer = document.createElement("textarea");
    document.body.appendChild(composer);
    composer.focus();

    const composerRef = createRef<HTMLTextAreaElement>();
    composerRef.current = composer;

    blurComposerOnPointerDown(
      { target: document.querySelector(".gap") } as Parameters<typeof blurComposerOnPointerDown>[0],
      composerRef,
    );

    expect(composer).toHaveFocus();
  });

  it("blurs the composer when tapping the transcript scroller", () => {
    document.body.innerHTML = '<main data-testid="session-thread"><p>Hi</p></main>';
    const composer = document.createElement("textarea");
    document.body.appendChild(composer);
    composer.focus();

    const composerRef = createRef<HTMLTextAreaElement>();
    composerRef.current = composer;
    const blur = vi.spyOn(composer, "blur");

    blurComposerOnPointerDown(
      { target: document.querySelector("p") } as Parameters<typeof blurComposerOnPointerDown>[0],
      composerRef,
    );

    expect(blur).toHaveBeenCalledOnce();
  });

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

  it("skips touchstart preventDefault on terminal toolbar buttons so clicks fire", () => {
    document.body.innerHTML =
      '<div data-testid="terminal-bottom-controls"><button type="button">Tab</button></div>';
    const root = document.querySelector("[data-testid='terminal-bottom-controls']") as HTMLElement;
    const button = document.querySelector("button")!;
    const event = new Event("touchstart", { cancelable: true, bubbles: true });
    Object.defineProperty(event, "target", { value: button, configurable: true });
    const preventDefault = vi.spyOn(event, "preventDefault");

    retainToolbarKeyboardOnCapture(root, event);

    expect(preventDefault).not.toHaveBeenCalled();
  });

  it("registers capture-phase toolbar listeners on terminal hotbar chrome", () => {
    document.body.innerHTML =
      '<div data-testid="terminal-bottom-controls"><span class="gap"></span></div>';
    const root = document.querySelector("[data-testid='terminal-bottom-controls']") as HTMLElement;
    const gap = document.querySelector(".gap")!;
    const addSpy = vi.spyOn(root, "addEventListener");

    const detach = attachToolbarKeyboardRetention(root);
    expect(addSpy).toHaveBeenCalledWith("touchstart", expect.any(Function), {
      capture: true,
      passive: false,
    });

    const event = new Event("touchstart", { cancelable: true, bubbles: true });
    Object.defineProperty(event, "target", { value: gap, configurable: true });
    const preventDefault = vi.spyOn(event, "preventDefault");
    root.dispatchEvent(event);
    expect(preventDefault).toHaveBeenCalledOnce();

    detach();
  });
});
