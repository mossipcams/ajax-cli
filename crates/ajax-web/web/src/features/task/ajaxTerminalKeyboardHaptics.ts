/** WebKit `switch` checkbox overlays — real user taps get iOS key haptics. */

const HIT_CLASS = "ajax-kb-haptic-hit";

export interface AjaxKeyboardHapticHandlers {
  onPress: (button: string) => void;
  onRelease: (button: string) => void;
}

function skbtnOf(button: Element): string {
  return button.getAttribute("data-skbtn") ?? "";
}

function isSpacer(button: string): boolean {
  return button === "{half}";
}

function attachHitTarget(
  button: HTMLElement,
  handlers: AjaxKeyboardHapticHandlers,
): void {
  const skbtn = skbtnOf(button);
  if (!skbtn || isSpacer(skbtn)) {
    button.querySelector(`.${HIT_CLASS}`)?.remove();
    return;
  }
  if (button.querySelector(`.${HIT_CLASS}`)) return;

  const input = document.createElement("input");
  input.type = "checkbox";
  input.className = HIT_CLASS;
  input.tabIndex = -1;
  input.setAttribute("switch", "");
  input.setAttribute("aria-hidden", "true");

  // Keep simple-keyboard's own listeners from also firing (bubble phase).
  for (const type of [
    "pointerdown",
    "pointerup",
    "pointercancel",
    "mousedown",
    "mouseup",
    "touchstart",
    "touchend",
    "click",
  ] as const) {
    input.addEventListener(
      type,
      (event) => {
        event.stopPropagation();
      },
      { passive: true },
    );
  }

  input.addEventListener("change", () => {
    input.checked = false;
    handlers.onPress(skbtn);
  });
  input.addEventListener("pointerup", () => {
    handlers.onRelease(skbtn);
  });
  input.addEventListener("pointercancel", () => {
    handlers.onRelease(skbtn);
  });

  button.appendChild(input);
}

/**
 * Overlay each rendered key with a WebKit `switch` checkbox so taps get
 * native iOS haptics, then forward press/release to Ajax handlers.
 */
export function attachAjaxKeyboardHaptics(
  root: HTMLElement,
  handlers: AjaxKeyboardHapticHandlers,
): () => void {
  const sync = () => {
    for (const button of root.querySelectorAll<HTMLElement>(".hg-button")) {
      attachHitTarget(button, handlers);
    }
  };

  sync();
  const observer = new MutationObserver(sync);
  observer.observe(root, { childList: true, subtree: true });

  return () => {
    observer.disconnect();
    for (const hit of root.querySelectorAll(`.${HIT_CLASS}`)) {
      hit.remove();
    }
  };
}
