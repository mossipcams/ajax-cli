import { useEffect, useRef, useState } from "react";
import Keyboard from "react-simple-keyboard";
import "react-simple-keyboard/build/css/index.css";
import { createHeldKeyRepeater } from "@/shared/lib/keyRepeat";
import { setSoftwareKeyboardOpen } from "@/shared/lib/viewport";
import { attachAjaxKeyboardHaptics } from "./ajaxTerminalKeyboardHaptics";
import {
  AJAX_KEYBOARD_BUTTON_THEME,
  AJAX_KEYBOARD_DISPLAY,
  AJAX_KEYBOARD_LAYOUT,
  type AjaxKeyboardLayoutName,
  isAjaxKeyboardSpacerButton,
  mapAjaxKeyboardButton,
  nextAjaxKeyboardLayout,
} from "./ajaxTerminalKeyboardLayout";

interface Props {
  onKey: (data: string) => void;
  onHide: () => void;
  /** Called after keyboard-open band geometry is applied / resized. */
  onGeometryChange?: () => void;
}

/**
 * iOS-sized on-screen keyboard for touch/narrow terminal typing.
 * Fixed to the page bottom; emits PTY bytes via `onKey`.
 * Key taps use WebKit switch-checkbox overlays for native haptics.
 */
export function AjaxTerminalKeyboard({ onKey, onHide, onGeometryChange }: Props) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [layoutName, setLayoutName] = useState<AjaxKeyboardLayoutName>("default");
  const bkspRepeaterRef = useRef<ReturnType<typeof createHeldKeyRepeater> | null>(null);
  const onKeyRef = useRef(onKey);
  onKeyRef.current = onKey;
  const onHideRef = useRef(onHide);
  onHideRef.current = onHide;
  const onGeometryChangeRef = useRef(onGeometryChange);
  onGeometryChangeRef.current = onGeometryChange;
  const layoutNameRef = useRef(layoutName);
  layoutNameRef.current = layoutName;

  const stopBkspRepeat = () => {
    bkspRepeaterRef.current?.stop();
    bkspRepeaterRef.current = null;
  };

  const startBkspRepeat = () => {
    stopBkspRepeat();
    const repeater = createHeldKeyRepeater({
      emit: () => onKeyRef.current("\x7f"),
      setTimeout,
      clearTimeout,
    });
    bkspRepeaterRef.current = repeater;
    repeater.start();
  };

  const handlePress = (button: string) => {
    if (isAjaxKeyboardSpacerButton(button)) return;
    const next = nextAjaxKeyboardLayout(layoutNameRef.current, button);
    if (next) {
      setLayoutName(next);
      return;
    }
    if (button === "{hide}") {
      onHideRef.current();
      return;
    }
    if (button === "{bksp}") {
      startBkspRepeat();
      return;
    }
    const payload = mapAjaxKeyboardButton(button);
    if (payload !== null) {
      onKeyRef.current(payload);
      if (layoutNameRef.current === "shift" && payload.length === 1) {
        setLayoutName("default");
      }
    }
  };

  const handleRelease = (button: string) => {
    if (button === "{bksp}") stopBkspRepeat();
  };

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const publishOpen = () => {
      const height = Math.ceil(root.getBoundingClientRect().height);
      setSoftwareKeyboardOpen(true, height);
      onGeometryChangeRef.current?.();
    };
    publishOpen();

    const resizeObserver = new ResizeObserver(publishOpen);
    resizeObserver.observe(root);
    const detachHaptics = attachAjaxKeyboardHaptics(root, {
      onPress: handlePress,
      onRelease: handleRelease,
    });

    return () => {
      detachHaptics();
      resizeObserver.disconnect();
      stopBkspRepeat();
      setSoftwareKeyboardOpen(false);
      onGeometryChangeRef.current?.();
    };
    // Handlers close over refs; attach once per mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      ref={rootRef}
      className="ajax-terminal-keyboard"
      data-testid="ajax-terminal-keyboard"
      onPointerDown={(event) => {
        // Keep terminal focus; stop fall-through reopen after dismiss.
        // Do not preventDefault on the haptic switch hit-target — WebKit only
        // fires switch haptics when the checkbox receives a real toggle.
        event.stopPropagation();
        const target = event.target as HTMLElement | null;
        if (target?.classList?.contains("ajax-kb-haptic-hit")) return;
        event.preventDefault();
      }}>
      <Keyboard
        layoutName={layoutName}
        layout={AJAX_KEYBOARD_LAYOUT}
        display={AJAX_KEYBOARD_DISPLAY}
        buttonTheme={AJAX_KEYBOARD_BUTTON_THEME}
        theme="hg-theme-default ajax-kb-theme"
        mergeDisplay
        useButtonTag
        preventMouseDownDefault
        disableCaretPositioning
        // Haptic overlays own press/release; keep library callbacks as a
        // non-touch fallback (pointer / accessibility paths).
        onKeyPress={handlePress}
        onKeyReleased={handleRelease}
      />
    </div>
  );
}
