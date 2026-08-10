/** WebKit `switch` overlays — sibling of each key button (never nested in <button>). */

const WRAP_CLASS = "ajax-kb-key-wrap";
const LABEL_CLASS = "ajax-kb-haptic-label";
const HIT_CLASS = "ajax-kb-haptic-hit";

export interface AjaxKeyboardHapticHandlers {
  onPress: (button: string) => void;
  onRelease: (button: string) => void;
}

const WRAP_MOD_CLASS: Record<string, string> = {
  "ajax-kb-half": "ajax-kb-wrap-half",
  "ajax-kb-enter": "ajax-kb-wrap-enter",
  "ajax-kb-bksp": "ajax-kb-wrap-bksp",
  "ajax-kb-done": "ajax-kb-wrap-done",
  "ajax-kb-mod": "ajax-kb-wrap-mod",
  "ajax-kb-space": "ajax-kb-wrap-space",
};

function skbtnOf(button: Element): string {
  return button.getAttribute("data-skbtn") ?? "";
}

function isSpacer(button: string): boolean {
  return button === "{half}";
}

function wrapClassFor(button: HTMLElement): string {
  for (const [buttonClass, wrapClass] of Object.entries(WRAP_MOD_CLASS)) {
    if (button.classList.contains(buttonClass)) return wrapClass;
  }
  return "";
}

function attachHitTarget(
  button: HTMLElement,
  handlers: AjaxKeyboardHapticHandlers,
): void {
  const skbtn = skbtnOf(button);
  if (!skbtn || isSpacer(skbtn)) return;
  if (button.closest(`.${WRAP_CLASS}`)) return;

  const parent = button.parentElement;
  if (!parent) return;

  const wrap = document.createElement("span");
  wrap.className = WRAP_CLASS;
  const mod = wrapClassFor(button);
  if (mod) wrap.classList.add(mod);

  parent.insertBefore(wrap, button);
  wrap.appendChild(button);

  const label = document.createElement("label");
  label.className = LABEL_CLASS;

  const input = document.createElement("input");
  input.type = "checkbox";
  input.className = HIT_CLASS;
  input.tabIndex = -1;
  input.setAttribute("switch", "");
  input.setAttribute("aria-hidden", "true");

  // Real user toggle → native switch haptic, then emit the key.
  // pointerup is a typing fallback if change does not fire on a stretched switch.
  let delivered = false;
  input.addEventListener("pointerdown", () => {
    delivered = false;
  });
  input.addEventListener("change", () => {
    input.checked = false;
    if (!delivered) {
      delivered = true;
      handlers.onPress(skbtn);
    }
  });
  input.addEventListener("pointerup", () => {
    if (!delivered) {
      delivered = true;
      handlers.onPress(skbtn);
    }
    handlers.onRelease(skbtn);
  });
  input.addEventListener("pointercancel", () => {
    handlers.onRelease(skbtn);
  });

  label.appendChild(input);
  wrap.appendChild(label);
}

function unwrapAll(root: HTMLElement): void {
  for (const wrap of [...root.querySelectorAll<HTMLElement>(`.${WRAP_CLASS}`)]) {
    const button = wrap.querySelector<HTMLElement>(".hg-button");
    if (button) wrap.parentElement?.insertBefore(button, wrap);
    wrap.remove();
  }
}

/**
 * Cover each key with a label+switch sibling (HapticButton pattern).
 * Input is never nested inside the <button> — that nesting blocked typing.
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
    unwrapAll(root);
  };
}
