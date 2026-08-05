import {
  useEffect,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent,
  type RefObject,
} from "react";
import {
  deleteBackward,
  insertAtSelection,
  moveCaret,
  type DraftSelection,
} from "./sessionDraftEdit";

const CONTROL_KEYS = [
  { label: "Esc", ariaLabel: "Escape", action: "esc" as const },
  { label: "Tab", ariaLabel: "Tab", action: "tab" as const },
  { label: "←", ariaLabel: "Left arrow", action: "left" as const, repeatable: true },
  { label: "↑", ariaLabel: "Up arrow", action: "up" as const, repeatable: true },
  { label: "↓", ariaLabel: "Down arrow", action: "down" as const, repeatable: true },
  { label: "→", ariaLabel: "Right arrow", action: "right" as const, repeatable: true },
];

const BACKSPACE = { label: "⌫", ariaLabel: "Backspace" };

const CTRL_ARM_TIMEOUT_MS = 4000;
const REPEAT_INITIAL_MS = 400;
const REPEAT_INTERVAL_MS = 50;

type KeyAction =
  | "esc"
  | "tab"
  | "left"
  | "right"
  | "up"
  | "down"
  | "backspace"
  | "paste"
  | "ctrl"
  | "mic";

export type SessionComposerKeysProps = {
  inputRef: RefObject<HTMLTextAreaElement | null>;
  draft: string;
  onDraftChange: (next: DraftSelection) => void;
  runStatus: "running" | "waiting" | null;
  onAbort: () => void;
  onDismissSheets: () => void;
  micArmed: boolean;
  micAriaLabel: string;
  micDisabled?: boolean;
  onToggleMic: () => void;
};

function readSelection(input: HTMLTextAreaElement | null, draft: string): DraftSelection {
  if (!input) {
    return { value: draft, selectionStart: draft.length, selectionEnd: draft.length };
  }
  return {
    value: draft,
    selectionStart: input.selectionStart ?? draft.length,
    selectionEnd: input.selectionEnd ?? draft.length,
  };
}

function applySelection(input: HTMLTextAreaElement | null, next: DraftSelection) {
  if (!input) return;
  requestAnimationFrame(() => {
    input.focus();
    input.setSelectionRange(next.selectionStart, next.selectionEnd);
  });
}

export default function SessionComposerKeys({
  inputRef,
  draft,
  onDraftChange,
  runStatus,
  onAbort,
  onDismissSheets,
  micArmed,
  micAriaLabel,
  micDisabled = false,
  onToggleMic,
}: SessionComposerKeysProps) {
  const toolbarPointerOwnedFocusRef = useRef(false);
  const [ctrlArmed, setCtrlArmed] = useState(false);
  const ctrlTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const repeatTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const repeatIntervalRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined);
  const draftRef = useRef(draft);
  draftRef.current = draft;
  const onAbortRef = useRef(onAbort);
  onAbortRef.current = onAbort;

  const clearRepeat = () => {
    if (repeatTimerRef.current) clearTimeout(repeatTimerRef.current);
    if (repeatIntervalRef.current) clearInterval(repeatIntervalRef.current);
    repeatTimerRef.current = undefined;
    repeatIntervalRef.current = undefined;
  };

  const disarmCtrl = () => {
    setCtrlArmed(false);
    if (ctrlTimerRef.current) clearTimeout(ctrlTimerRef.current);
    ctrlTimerRef.current = undefined;
  };

  const armCtrl = () => {
    setCtrlArmed(true);
    if (ctrlTimerRef.current) clearTimeout(ctrlTimerRef.current);
    ctrlTimerRef.current = setTimeout(() => {
      setCtrlArmed(false);
      ctrlTimerRef.current = undefined;
    }, CTRL_ARM_TIMEOUT_MS);
  };

  useEffect(() => {
    return () => {
      clearRepeat();
      if (ctrlTimerRef.current) clearTimeout(ctrlTimerRef.current);
    };
  }, []);

  // Soft-keyboard "C" while Ctrl is armed → abort (terminal Ctrl+C).
  useEffect(() => {
    if (!ctrlArmed) return;
    const input = inputRef.current;
    if (!input) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "c" || event.key === "C") {
        event.preventDefault();
        disarmCtrl();
        onAbortRef.current();
      }
    };
    input.addEventListener("keydown", onKeyDown);
    return () => input.removeEventListener("keydown", onKeyDown);
  }, [ctrlArmed, inputRef]);

  const composerOwnedFocus = () => document.activeElement === inputRef.current;

  const onToolbarPointerDown = (event: PointerEvent) => {
    event.preventDefault();
    toolbarPointerOwnedFocusRef.current = composerOwnedFocus();
  };

  const consumeToolbarPointerOwnedFocus = (event: MouseEvent): boolean => {
    const owned = toolbarPointerOwnedFocusRef.current && event.detail !== 0;
    toolbarPointerOwnedFocusRef.current = false;
    return owned;
  };

  const refocusIfOwned = (owned: boolean) => {
    if (owned) inputRef.current?.focus();
  };

  const mutateDraft = (mutator: (state: DraftSelection) => DraftSelection) => {
    const input = inputRef.current;
    const next = mutator(readSelection(input, draftRef.current));
    onDraftChange(next);
    applySelection(input, next);
  };

  const runAction = (action: KeyAction) => {
    if (action === "esc") {
      if (runStatus === "running") {
        onAbort();
      } else {
        inputRef.current?.blur();
        onDismissSheets();
      }
      return;
    }
    if (action === "tab") {
      mutateDraft((state) => insertAtSelection(state, "\t"));
      return;
    }
    if (action === "left" || action === "right" || action === "up" || action === "down") {
      mutateDraft((state) => moveCaret(state, action));
      return;
    }
    if (action === "backspace") {
      mutateDraft((state) => deleteBackward(state));
      return;
    }
    if (action === "paste") {
      void (async () => {
        try {
          const text = await navigator.clipboard.readText();
          if (text) mutateDraft((state) => insertAtSelection(state, text));
        } catch {
          // Clipboard denied — OS paste into the textarea still works.
        }
      })();
      return;
    }
    if (action === "ctrl") {
      if (ctrlArmed) disarmCtrl();
      else armCtrl();
      return;
    }
    if (action === "mic") {
      onToggleMic();
    }
  };

  const onControlKeyClick = (event: MouseEvent, action: KeyAction, repeatable: boolean) => {
    const ownedFocus = consumeToolbarPointerOwnedFocus(event);
    if (repeatable && event.detail === 0) {
      runAction(action);
      refocusIfOwned(ownedFocus);
      return;
    }
    runAction(action);
    refocusIfOwned(ownedFocus);
  };

  const onRepeatablePointerDown = (event: PointerEvent, action: KeyAction) => {
    onToolbarPointerDown(event);
    clearRepeat();
    runAction(action);
    repeatTimerRef.current = setTimeout(() => {
      repeatIntervalRef.current = setInterval(() => runAction(action), REPEAT_INTERVAL_MS);
    }, REPEAT_INITIAL_MS);
  };

  return (
    <div data-testid="ajax-web-session-bottom-controls">
      <div className="terminal-keys ajax-web-session-keys" role="toolbar" aria-label="Session keys">
        {CONTROL_KEYS.map((key) => {
          const repeatable = Boolean(key.repeatable);
          return (
            <button
              key={key.label}
              type="button"
              className="terminal-key"
              aria-label={key.ariaLabel}
              data-testid={`ajax-web-session-key-${key.action}`}
              onPointerDown={(event) => {
                if (repeatable) {
                  onRepeatablePointerDown(event, key.action);
                  return;
                }
                onToolbarPointerDown(event);
              }}
              onPointerUp={repeatable ? clearRepeat : undefined}
              onPointerCancel={repeatable ? clearRepeat : undefined}
              onLostPointerCapture={repeatable ? clearRepeat : undefined}
              onClick={(event) => onControlKeyClick(event, key.action, repeatable)}
            >
              {key.label}
            </button>
          );
        })}
        <button
          type="button"
          className={`terminal-key${ctrlArmed ? " is-armed" : ""}`}
          aria-label="Control modifier"
          aria-pressed={ctrlArmed}
          data-testid="ajax-web-session-key-ctrl"
          onPointerDown={onToolbarPointerDown}
          onClick={(event) => {
            const ownedFocus = consumeToolbarPointerOwnedFocus(event);
            runAction("ctrl");
            refocusIfOwned(ownedFocus);
          }}
        >
          Ctrl
          {ctrlArmed ? <span className="terminal-key-armed-dot" aria-hidden="true" /> : null}
        </button>
        <button
          type="button"
          className="terminal-key"
          aria-label="Paste"
          data-testid="ajax-web-session-key-paste"
          onPointerDown={onToolbarPointerDown}
          onClick={(event) => {
            const ownedFocus = consumeToolbarPointerOwnedFocus(event);
            runAction("paste");
            refocusIfOwned(ownedFocus);
          }}
        >
          Paste
        </button>
        <button
          type="button"
          className="terminal-key"
          aria-label={BACKSPACE.ariaLabel}
          data-testid="ajax-web-session-key-backspace"
          onPointerDown={(event) => onRepeatablePointerDown(event, "backspace")}
          onPointerUp={clearRepeat}
          onPointerCancel={clearRepeat}
          onLostPointerCapture={clearRepeat}
          onClick={(event) => onControlKeyClick(event, "backspace", true)}
        >
          {BACKSPACE.label}
        </button>
        <button
          type="button"
          className={`terminal-key${micArmed ? " is-armed" : ""}`}
          aria-label={micArmed ? "Stop voice input" : micAriaLabel}
          title={micArmed ? "Stop voice input" : micAriaLabel}
          data-testid="ajax-web-session-key-mic"
          disabled={micDisabled}
          onPointerDown={onToolbarPointerDown}
          onClick={(event) => {
            const ownedFocus = consumeToolbarPointerOwnedFocus(event);
            runAction("mic");
            refocusIfOwned(ownedFocus);
          }}
        >
          Mic
        </button>
      </div>
    </div>
  );
}
