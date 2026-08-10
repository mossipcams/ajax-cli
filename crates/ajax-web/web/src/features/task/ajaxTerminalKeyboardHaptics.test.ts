import { afterEach, describe, expect, it, vi } from "vitest";
import { attachAjaxKeyboardHaptics } from "./ajaxTerminalKeyboardHaptics";

function makeKey(skbtn: string, extraClass?: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.className = extraClass ? `hg-button ${extraClass}` : "hg-button";
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

  it("wraps each key with a sibling label+switch (not nested in the button)", () => {
    const root = document.createElement("div");
    root.appendChild(makeKey("a"));
    document.body.appendChild(root);

    const detach = attachAjaxKeyboardHaptics(root, {
      onPress: () => {},
      onRelease: () => {},
    });

    const wrap = root.querySelector(".ajax-kb-key-wrap");
    const button = root.querySelector(".hg-button");
    const hit = root.querySelector<HTMLInputElement>(".ajax-kb-haptic-hit");
    expect(wrap).not.toBeNull();
    expect(button?.parentElement).toBe(wrap);
    expect(hit?.parentElement?.classList.contains("ajax-kb-haptic-label")).toBe(true);
    expect(button?.contains(hit!)).toBe(false);
    expect(hit?.type).toBe("checkbox");
    expect(hit?.getAttribute("switch")).toBe("");
    detach();
  });

  it("fires onPress from checkbox change and skips spacer keys", () => {
    const root = document.createElement("div");
    root.appendChild(makeKey("a"));
    root.appendChild(makeKey("{half}", "ajax-kb-half"));
    document.body.appendChild(root);

    const onPress = vi.fn();
    const onRelease = vi.fn();
    attachAjaxKeyboardHaptics(root, { onPress, onRelease });

    expect(root.querySelector('[data-skbtn="{half}"]')?.closest(".ajax-kb-key-wrap")).toBeNull();

    const hit = root.querySelector<HTMLInputElement>(".ajax-kb-haptic-hit");
    expect(hit).not.toBeNull();
    hit!.checked = true;
    hit!.dispatchEvent(new Event("change"));
    expect(onPress).toHaveBeenCalledWith("a");
    expect(hit!.checked).toBe(false);
  });
});
