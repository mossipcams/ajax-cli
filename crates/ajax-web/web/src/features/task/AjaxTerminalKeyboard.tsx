import { useEffect, useRef, useState } from "react";
import Keyboard from "react-simple-keyboard";
import "react-simple-keyboard/build/css/index.css";
import { createHeldKeyRepeater } from "@/shared/lib/keyRepeat";
import { setSoftwareKeyboardOpen } from "@/shared/lib/viewport";
import {
  AJAX_KEYBOARD_BUTTON_THEME,
  AJAX_KEYBOARD_DISPLAY,
  AJAX_KEYBOARD_LAYOUT,
  type AjaxKeyboardLayoutName,
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
 * Compact on-screen keyboard for touch/narrow terminal typing.
 * Emits PTY bytes via `onKey`; does not own an input buffer.
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

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const publishOpen = () => {
      setSoftwareKeyboardOpen(true);
      onGeometryChangeRef.current?.();
    };
    publishOpen();

    const observer = new ResizeObserver(publishOpen);
    observer.observe(root);
    return () => {
      observer.disconnect();
      bkspRepeaterRef.current?.stop();
      bkspRepeaterRef.current = null;
      setSoftwareKeyboardOpen(false);
      onGeometryChangeRef.current?.();
    };
  }, []);

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

  const onKeyPress = (button: string) => {
    const next = nextAjaxKeyboardLayout(layoutName, button);
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
      // One-shot shift: return to lowercase after a character.
      if (layoutName === "shift" && payload.length === 1) {
        setLayoutName("default");
      }
    }
  };

  const onKeyReleased = (button: string) => {
    if (button === "{bksp}") stopBkspRepeat();
  };

  return (
    <div
      ref={rootRef}
      className="ajax-terminal-keyboard"
      data-testid="ajax-terminal-keyboard"
      onPointerDown={(event) => event.preventDefault()}>
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
        onKeyPress={onKeyPress}
        onKeyReleased={onKeyReleased}
      />
    </div>
  );
}
