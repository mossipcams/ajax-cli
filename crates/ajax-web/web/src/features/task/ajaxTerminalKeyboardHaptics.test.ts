import { afterEach, describe, expect, it, vi } from "vitest";
import { attachAjaxKeyboardHaptics } from "./ajaxTerminalKeyboardHaptics";

function makeKey(skbtn: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = "hg-button";
  button.setAttribute("data-skbtn", skbtn);
  const label = document.createElement("span");
  label.textContent = skbtn;
  button.appendChild(label);
  return button;
}

describe("attachAjaxKeyboardHaptics", () => {
  afterEach(() => {
    document.body.replaceChildren();
  });

  it("overlays a WebKit switch checkbox on each key", () => {
    const root = document.createElement("div");
    root.appendChild(makeKey("a"));
    document.body.appendChild(root);

    const detach = attachAjaxKeyboardHaptics(root, {
      onPress: () => {},
      onRelease: () => {},
    });

    const hit = root.querySelector<HTMLInputElement>(".ajax-kb-haptic-hit");
    expect(hit).not.toBeNull();
    expect(hit?.type).toBe("checkbox");
    expect(hit?.getAttribute("switch")).toBe("");
    detach();
  });

  it("fires onPress from checkbox change and skips spacer keys", () => {
    const root = document.createElement("div");
    root.appendChild(makeKey("a"));
    root.appendChild(makeKey("{half}"));
    document.body.appendChild(root);

    const onPress = vi.fn();
    const onRelease = vi.fn();
    attachAjaxKeyboardHaptics(root, { onPress, onRelease });

    expect(root.querySelector('[data-skbtn="{half}"] .ajax-kb-haptic-hit')).toBeNull();

    const hit = root.querySelector<HTMLInputElement>(".ajax-kb-haptic-hit");
    expect(hit).not.toBeNull();
    hit!.checked = true;
    hit!.dispatchEvent(new Event("change"));
    expect(onPress).toHaveBeenCalledWith("a");
    expect(hit!.checked).toBe(false);
  });
});
