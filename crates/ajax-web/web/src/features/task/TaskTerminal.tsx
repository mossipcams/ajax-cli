import { useState, useEffect, useEffectEvent, useRef } from "react";
import "@xterm/xterm/css/xterm.css";
import { copyText, readPasteText } from "@/shared/lib/clipboard";
import type { TerminalConnection, TerminalConnectionStatus } from "@/shared/lib/terminalConnection";
import { createHeldKeyRepeater } from "@/shared/lib/keyRepeat";
import { FloatingContextMenu } from "@/shared/ui/FloatingContextMenu";
import { mountTaskTerminalSession } from "./mountTaskTerminalSession";
import { useTaskTerminalSpeech } from "./useTaskTerminalSpeech";
import type { Terminal } from "@xterm/xterm";
import { attachTerminalAddons } from "@/shared/lib/terminalAddons";
import type { TerminalLinkService } from "@/shared/lib/terminalLinkService";

interface Props {
  handle: string;
}

// iOS only starts its hold-to-delete repeat loop when the focused field has
// deletable content, so the xterm helper textarea always carries a sentinel.
const BACKSPACE_SENTINEL = "\u200B";

const seedBackspaceSentinel = (input: HTMLTextAreaElement | null) => {
  if (input && !input.value.includes(BACKSPACE_SENTINEL)) {
    input.value = BACKSPACE_SENTINEL;
  }
};

// Module scope on purpose: registered from hardenMobileTextarea and removed in
// the effect cleanup, which see different render closures. One stable identity
// is the only way both sides name the same function.
const seedSentinelFromFocus = (event: Event) => {
  const input = event.currentTarget;
  seedBackspaceSentinel(input instanceof HTMLTextAreaElement ? input : null);
};

export default function TaskTerminal({ handle }: Props) {
  const hostElRef = useRef<HTMLDivElement | null>(null);
  const interactionElRef = useRef<HTMLDivElement | null>(null);
  const spacerElRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | undefined>(undefined);
  const connectionRef = useRef<TerminalConnection | undefined>(undefined);
  const schedulePostLayoutRef = useRef<((discreteIntent?: boolean) => void) | undefined>(
    undefined,
  );
  const resetResizeDedupeRef = useRef<(() => void) | undefined>(undefined);
  const jumpToBottomRef = useRef<(() => void) | undefined>(undefined);
  const inertedElementsRef = useRef<HTMLElement[]>([]);
  const copyNoticeTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const ctrlTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const expandSettleFrame1Ref = useRef(0);
  const expandSettleFrame2Ref = useRef(0);
  const expandSettleTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pasteFallbackOwnedFocusRef = useRef(false);
  const toolbarPointerOwnedFocusRef = useRef(false);
  const heldKeyRepeaterRef = useRef<ReturnType<typeof createHeldKeyRepeater> | null>(null);
  const toolbarRepeatHandledRef = useRef(false);
  const toolbarRepeatOwnedFocusRef = useRef(false);
  const ctrlArmedRef = useRef(false);

  const [status, setStatus] = useState<TerminalConnectionStatus>("connecting");
  const [statusDetail, setStatusDetail] = useState("");
  const [ctrlArmed, setCtrlArmed] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [hasUnseenOutput, setHasUnseenOutput] = useState(false);
  const [pasteFallbackOpen, setPasteFallbackOpen] = useState(false);
  const [pasteFallbackText, setPasteFallbackText] = useState("");
  const [pasteNotice, setPasteNotice] = useState("");
  const [copyOverlayText, setCopyOverlayText] = useState("");
  const [copyNotice, setCopyNotice] = useState("");
  const [copyFallbackOpen, setCopyFallbackOpen] = useState(false);
  const [copyFallbackText, setCopyFallbackText] = useState("");
  const [linkMenu, setLinkMenu] = useState<{
    url: string;
    x: number;
    y: number;
  } | null>(null);
  const linkServiceRef = useRef<TerminalLinkService | undefined>(undefined);
  const terminalSnapshotRef = useRef<
    ReturnType<typeof attachTerminalAddons>["snapshot"] | undefined
  >(undefined);

  const statusVisible = status !== "connected" || statusDetail.length > 0;
  const showReconnect = status === "reconnecting" || status === "unavailable";

  const isPhoneTerminalLayout = () =>
    window.matchMedia("(max-width: 767px), (pointer: coarse) and (max-height: 500px)").matches;

  const clearExpandedInert = () => {
    for (const el of inertedElementsRef.current) {
      el.inert = false;
    }
    inertedElementsRef.current = [];
  };

  const applyExpandedInert = () => {
    clearExpandedInert();
    if (!isPhoneTerminalLayout()) return;

    const panel = hostElRef.current?.closest<HTMLElement>('[data-testid="task-terminal-panel"]');
    const taskDetail = panel?.closest<HTMLElement>(".task-detail");
    const next: HTMLElement[] = [];

    if (taskDetail && panel) {
      for (const child of taskDetail.children) {
        if (child instanceof HTMLElement && !child.contains(panel)) {
          next.push(child);
        }
      }
    }

    for (const el of document.querySelectorAll<HTMLElement>(
      ".cockpit-chrome, .bottom-nav, .result-panel",
    )) {
      next.push(el);
    }

    for (const el of next) {
      if (el.inert) continue;
      el.inert = true;
      inertedElementsRef.current.push(el);
    }
  };

  const syncExpandedInert = (active: boolean) => {
    if (active) applyExpandedInert();
    else clearExpandedInert();
  };


  const EXPAND_REWRAP_MS = 280;
  const EXPANDED_CLASS = "terminal-expanded";

  const CONTROL_KEYS = [
    { label: "Esc", ariaLabel: "Escape", data: "\x1b" },
    { label: "Tab", ariaLabel: "Tab", data: "\t" },
    { label: "←", ariaLabel: "Left arrow", data: "\x1b[D" },
    { label: "↑", ariaLabel: "Up arrow", data: "\x1b[A" },
    { label: "↓", ariaLabel: "Down arrow", data: "\x1b[B" },
    { label: "→", ariaLabel: "Right arrow", data: "\x1b[C" },
  ];

  const BACKSPACE_KEY = { label: "⌫", ariaLabel: "Backspace", data: "\x7f" };

  const REPEATABLE_KEY_DATA = new Set([
    "\x7f",
    "\x1b[D",
    "\x1b[A",
    "\x1b[B",
    "\x1b[C",
  ]);

  const isRepeatableKey = (data: string) => REPEATABLE_KEY_DATA.has(data);

  const CTRL_ARM_TIMEOUT_MS = 4000;

  const STATUS_LABELS: Record<TerminalConnectionStatus, string> = {
    connecting: "Connecting…",
    connected: "Connected",
    reconnecting: "Reconnecting…",
    unavailable: "Unavailable",
  };

  const cancelExpandSettle = () => {
    if (expandSettleFrame1Ref.current) {
      cancelAnimationFrame(expandSettleFrame1Ref.current);
      expandSettleFrame1Ref.current = 0;
    }
    if (expandSettleFrame2Ref.current) {
      cancelAnimationFrame(expandSettleFrame2Ref.current);
      expandSettleFrame2Ref.current = 0;
    }
    if (expandSettleTimerRef.current) {
      clearTimeout(expandSettleTimerRef.current);
      expandSettleTimerRef.current = undefined;
    }
  };

  const scheduleBandSettle = () => {
    cancelExpandSettle();
    schedulePostLayoutRef.current?.(true);
    expandSettleFrame1Ref.current = requestAnimationFrame(() => {
      expandSettleFrame1Ref.current = 0;
      schedulePostLayoutRef.current?.(true);
      expandSettleFrame2Ref.current = requestAnimationFrame(() => {
        expandSettleFrame2Ref.current = 0;
        schedulePostLayoutRef.current?.(true);
      });
    });
    expandSettleTimerRef.current = setTimeout(() => {
      expandSettleTimerRef.current = undefined;
      schedulePostLayoutRef.current?.(true);
    }, EXPAND_REWRAP_MS);
  };

  const disarmCtrl = () => {
    ctrlArmedRef.current = false;
    setCtrlArmed(false);
    if (ctrlTimerRef.current) {
      clearTimeout(ctrlTimerRef.current);
      ctrlTimerRef.current = undefined;
    }
  };

  const toggleCtrl = () => {
    if (ctrlArmedRef.current) {
      disarmCtrl();
      return;
    }
    ctrlArmedRef.current = true;
    setCtrlArmed(true);
    if (ctrlTimerRef.current) clearTimeout(ctrlTimerRef.current);
    ctrlTimerRef.current = setTimeout(disarmCtrl, CTRL_ARM_TIMEOUT_MS);
  };

  const controlModify = (data: string): string => {
    if (data.length === 1) {
      const code = data.toLowerCase().charCodeAt(0);
      if (code >= 97 && code <= 122) return String.fromCharCode(code - 96);
    }
    // ANSI CSI cursor sequences are the point of this match.
    // eslint-disable-next-line no-control-regex -- CSI ESC must appear in the pattern
    const cursor = /^\x1b\[([ABCD])$/.exec(data);
    if (cursor) return `\x1b[1;5${cursor[1]}`;
    return data;
  };

  const consumeCtrl = (data: string): string => {
    if (!ctrlArmedRef.current) return data;
    disarmCtrl();
    return controlModify(data);
  };

  const sendKey = (data: string) => {
    if (!connectionRef.current?.isOpen()) return;
    connectionRef.current.sendInput(data);
  };

  const stopHeldKeyRepeat = () => {
    heldKeyRepeaterRef.current?.stop();
    heldKeyRepeaterRef.current = null;
  };

  const PASTE_DISCONNECTED_NOTICE = "Terminal disconnected — paste kept below.";

  const pasteToPty = (text: string): boolean => {
    if (!text || !connectionRef.current?.isOpen()) return false;
    const payload = termRef.current?.modes.bracketedPasteMode
      ? `\x1b[200~${text}\x1b[201~`
      : text;
    connectionRef.current.sendInput(payload);
    return true;
  };

  const termTextarea = (): HTMLTextAreaElement | null => {
    const el = termRef.current?.element?.querySelector("textarea.xterm-helper-textarea");
    return el instanceof HTMLTextAreaElement ? el : null;
  };

  const seedTermSentinel = () => {
    seedBackspaceSentinel(termTextarea());
  };

  const hardenMobileTextarea = () => {
    const input = termTextarea();
    if (!input) return;
    input.setAttribute("autocapitalize", "off");
    input.setAttribute("autocorrect", "off");
    input.setAttribute("autocomplete", "off");
    input.setAttribute("spellcheck", "false");
    input.style.fontSize = "16px";
    input.style.position = "absolute";
    input.style.bottom = "0";
    input.style.height = "44px";
    input.style.width = "100%";
    input.style.opacity = "0.01";
    input.style.setProperty("clip-path", "none");
    input.style.setProperty("-webkit-clip-path", "none");
    input.style.setProperty("clip", "auto");
    input.style.color = "transparent";
    input.style.setProperty("-webkit-text-fill-color", "transparent");
    input.style.caretColor = "transparent";
    seedBackspaceSentinel(input);
    input.addEventListener("focus", seedSentinelFromFocus);
  };

  // Measured on an iOS 26 Simulator: a held Delete repeats deleteContentBackward
  // at ~100ms, then escalates to deleteWordBackward after ~800ms. Ignoring the
  // escalation strands the rest of the hold.
  const deleteInputPayload = (inputType: string): string | null => {
    if (inputType === "deleteWordBackward") return "\x17";
    if (inputType === "deleteContentBackward" || inputType === "deleteContentForward") {
      return "\x7f";
    }
    return null;
  };

  // Backspace is the one key we leave uncancelled (cancelling it kills the iOS
  // hold-to-delete repeat), so WebKit really edits the helper textarea and then
  // reveals the caret — after the input event, measured on mobile-webkit as
  // input → selectionchange → scroll. .terminal-host is sticky, so the
  // textarea's layout position sits near the top of the spacer-extended scroll
  // range and the reveal drags the wrap, and the whole terminal with it, up into
  // scrollback. Pin the offset over the edit and put it back from the scroll
  // event the reveal fires, before that scroll can drive the PTY viewport.
  const pinnedScrollTopRef = useRef<number | null>(null);

  const pinInteractionScroll = () => {
    pinnedScrollTopRef.current = interactionElRef.current?.scrollTop ?? null;
  };

  const clearInteractionScrollPin = () => {
    pinnedScrollTopRef.current = null;
  };

  /** True when this scroll was the caret reveal and has been undone. */
  const restorePinnedInteractionScroll = (): boolean => {
    const wrap = interactionElRef.current;
    const pinned = pinnedScrollTopRef.current;
    if (!wrap || pinned === null) return false;
    clearInteractionScrollPin();
    if (wrap.scrollTop === pinned) return false;
    wrap.scrollTop = pinned;
    return true;
  };

  // Dedup paste vs beforeinput(insertFromPaste) on browsers that fire both.
  const pasteHandledAtRef = useRef(0);
  // Empty sync clipboardData: block xterm's empty clear, recover from input.
  const pasteExpectRef = useRef(false);
  const claimPasteHandle = (): boolean => {
    const now = performance.now();
    if (now - pasteHandledAtRef.current < 50) return false;
    pasteHandledAtRef.current = now;
    return true;
  };

  const sendPastedText = (text: string) => {
    if (!text || !claimPasteHandle()) return;
    seedTermSentinel();
    pasteThroughTerm(text);
  };

  const onTextareaPasteBeforeInput = (event: InputEvent) => {
    // iOS keyboard "Paste" / QuickType link often uses beforeinput with the
    // URL in event.data and an empty ClipboardEvent.clipboardData.
    if (
      event.inputType !== "insertFromPaste" &&
      event.inputType !== "insertFromPasteAsQuotation"
    ) {
      return;
    }
    const text =
      (event.dataTransfer ? readPasteText(event.dataTransfer) : "") ||
      (event.data ?? "").trim();
    if (!text) return;
    event.preventDefault();
    sendPastedText(text);
  };

  const onTextareaBeforeInput = (event: InputEvent) => {
    const payload = deleteInputPayload(event.inputType);
    if (payload) {
      pinInteractionScroll();
      // No preventDefault: cancelling here also cancels the iOS repeat loop.
      sendKey(consumeCtrl(payload));
      return;
    }
    onTextareaPasteBeforeInput(event);
  };

  // Reseed here, never from a beforeinput microtask: the microtask checkpoint
  // runs *before* the browser applies the deletion, so it always sees the
  // sentinel still present, does nothing, and leaves the textarea empty for the
  // next repeat tick.
  const onTextareaInput = (event: Event) => {
    const inputType = (event as InputEvent).inputType ?? "";
    if (inputType === "insertText") {
      pasteExpectRef.current = false;
      return;
    }
    if (inputType.startsWith("delete")) {
      pasteExpectRef.current = false;
      seedTermSentinel();
      // The reveal scroll lands in this frame; drop the pin after it so it can
      // never swallow a later finger scroll.
      requestAnimationFrame(clearInteractionScrollPin);
      return;
    }
    if (
      inputType === "insertFromPaste" ||
      inputType === "insertFromPasteAsQuotation" ||
      pasteExpectRef.current
    ) {
      const textarea = event.currentTarget;
      if (textarea instanceof HTMLTextAreaElement) {
        const raw = textarea.value.replaceAll(BACKSPACE_SENTINEL, "");
        pasteExpectRef.current = false;
        // Force-clear: seedBackspaceSentinel no-ops when ZWS is still present
        // beside the pasted text.
        textarea.value = BACKSPACE_SENTINEL;
        sendPastedText(raw);
      }
    }
  };

  const onTextareaPaste = (event: ClipboardEvent) => {
    const text = readPasteText(event.clipboardData);
    if (text) {
      // Only cancel once we have payload — empty preventDefault swallowed all
      // Safari pastes when clipboardData was inaccessible synchronously.
      event.preventDefault();
      event.stopImmediatePropagation();
      sendPastedText(text);
      return;
    }

    // beforeinput may already have owned this paste gesture.
    if (performance.now() - pasteHandledAtRef.current < 50) {
      event.preventDefault();
      event.stopImmediatePropagation();
      return;
    }

    // Sync formats empty: block xterm's empty clear, let the browser insert.
    pasteExpectRef.current = true;
    event.stopImmediatePropagation();
  };

  const termOwnedFocus = (): boolean => {
    const textarea = termTextarea();
    return textarea !== null && document.activeElement === textarea;
  };

  const refocusTermIfOwned = (ownedFocus: boolean) => {
    if (!ownedFocus) return;
    const textarea = termTextarea();
    if (textarea) {
      textarea.focus({ preventScroll: true });
      return;
    }
    termRef.current?.focus();
  };

  const blurTerm = () => {
    termTextarea()?.blur();
  };

  const onToolbarPointerDown = (event: React.PointerEvent) => {
    event.preventDefault();
    toolbarPointerOwnedFocusRef.current = termOwnedFocus();
  };

  const consumeToolbarPointerOwnedFocus = (event: React.MouseEvent): boolean => {
    const owned = toolbarPointerOwnedFocusRef.current && event.detail !== 0;
    toolbarPointerOwnedFocusRef.current = false;
    return owned;
  };

  const endRepeatableKeyPress = () => {
    stopHeldKeyRepeat();
    refocusTermIfOwned(toolbarRepeatOwnedFocusRef.current);
    toolbarRepeatOwnedFocusRef.current = false;
  };

  const onRepeatableKeyPointerDown = (
    event: React.PointerEvent<HTMLButtonElement>,
    data: string,
  ) => {
    onToolbarPointerDown(event);
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    toolbarRepeatOwnedFocusRef.current = toolbarPointerOwnedFocusRef.current;
    toolbarRepeatHandledRef.current = true;
    const payload = consumeCtrl(data);
    stopHeldKeyRepeat();
    const repeater = createHeldKeyRepeater({
      emit: () => {
        if (!connectionRef.current?.isOpen()) {
          stopHeldKeyRepeat();
          return;
        }
        sendKey(payload);
      },
      isActive: () => connectionRef.current?.isOpen() ?? false,
      setTimeout: window.setTimeout.bind(window),
      clearTimeout: window.clearTimeout.bind(window),
    });
    heldKeyRepeaterRef.current = repeater;
    repeater.start();
  };

  const onRepeatableKeyPointerEnd = () => {
    if (!toolbarRepeatHandledRef.current) return;
    endRepeatableKeyPress();
    toolbarRepeatHandledRef.current = false;
  };

  const onControlKeyClick = (
    event: React.MouseEvent<HTMLButtonElement>,
    data: string,
    repeatable: boolean,
  ) => {
    const ownedFocus = consumeToolbarPointerOwnedFocus(event);
    // Repeatable keys already emit once from onRepeatableKeyPointerDown, so the
    // trailing pointer/touch click must never send again. iOS can deliver that
    // synthetic click after a timing flag would have expired (that race sent the
    // arrow twice and skipped a line), so key off event.detail — 0 means a
    // keyboard activation, which had no pointerdown emit and must send once.
    if (repeatable && event.detail !== 0) {
      refocusTermIfOwned(ownedFocus);
      return;
    }
    sendKey(consumeCtrl(data));
    refocusTermIfOwned(ownedFocus);
  };

  const openPasteFallback = (ownedFocus: boolean, notice: string, text = "") => {
    pasteFallbackOwnedFocusRef.current = ownedFocus;
    setPasteNotice(notice);
    setPasteFallbackText(text);
    setPasteFallbackOpen(true);
  };

  const retainUnsentPaste = (text: string, ownedFocus: boolean) => {
    openPasteFallback(ownedFocus, PASTE_DISCONNECTED_NOTICE, text);
  };

  const dismissPasteFallback = (): boolean => {
    const ownedFocus = pasteFallbackOwnedFocusRef.current;
    setPasteFallbackOpen(false);
    setPasteFallbackText("");
    setPasteNotice("");
    pasteFallbackOwnedFocusRef.current = false;
    return ownedFocus;
  };

  const closePasteFallback = () => {
    refocusTermIfOwned(dismissPasteFallback());
  };

  const pasteThroughTerm = (text: string, ownedFocus = true): boolean => {
    if (!text || !termRef.current) return false;
    if (!pasteToPty(text)) {
      retainUnsentPaste(text, ownedFocus);
      return false;
    }
    refocusTermIfOwned(ownedFocus);
    return true;
  };

  const requestPaste = async (ownedFocus: boolean) => {
    try {
      const readText = navigator.clipboard?.readText;
      if (!readText) {
        openPasteFallback(ownedFocus, "Clipboard unavailable — paste below.");
        return;
      }
      const text = await readText.call(navigator.clipboard);
      if (!text) {
        refocusTermIfOwned(ownedFocus);
        return;
      }
      pasteThroughTerm(text, ownedFocus);
    } catch {
      openPasteFallback(ownedFocus, "Clipboard denied — paste below.");
    }
  };

  const sendPasteFallback = () => {
    const text = pasteFallbackText;
    const ownedFocus = pasteFallbackOwnedFocusRef.current;
    if (!text) {
      closePasteFallback();
      return;
    }
    if (!pasteToPty(text)) {
      setPasteNotice(PASTE_DISCONNECTED_NOTICE);
      return;
    }
    dismissPasteFallback();
    refocusTermIfOwned(ownedFocus);
  };

  const cancelPasteFallback = () => {
    closePasteFallback();
  };

  const {
    speechModel,
    pauseCountdownSeconds,
    micAriaLabel,
    micArmed,
    toggleMic,
    cancelSpeechInput,
    cancelSpeechTransport,
  } = useTaskTerminalSpeech({
    handle,
    termRef,
    connectionRef,
    pasteThroughTerm,
  });

  const syncCopyOverlay = () => {
    const selection = termRef.current?.getSelection() ?? "";
    setCopyOverlayText(selection);
    if (!selection && !copyNoticeTimerRef.current) setCopyNotice("");
  };

  const dismissCopyFallback = () => {
    setCopyFallbackOpen(false);
    setCopyFallbackText("");
  };

  const copySelection = async () => {
    const text = copyOverlayText || termRef.current?.getSelection() || "";
    if (!text) return;
    const copied = await copyText(text);
    if (copied) {
      if (copyNoticeTimerRef.current) clearTimeout(copyNoticeTimerRef.current);
      setCopyNotice("Copied");
      copyNoticeTimerRef.current = setTimeout(() => {
        setCopyNotice("");
        copyNoticeTimerRef.current = undefined;
      }, 1500);
      termRef.current?.clearSelection();
      setCopyOverlayText("");
      return;
    }
    setCopyFallbackText(text);
    setCopyFallbackOpen(true);
  };

  const requestReconnect = () => {
    connectionRef.current?.reconnectNow();
  };

  const toggleExpanded = () => {
    const entering = !expanded;
    setExpanded(entering);
    document.documentElement.classList.toggle(EXPANDED_CLASS, entering);
    syncExpandedInert(entering);
    resetResizeDedupeRef.current?.();
    if (!entering) {
      blurTerm();
      // Exit while keyboard-open used to call discreteIntent=false, which is a
      // no-op under the fit freeze — inline band never refit. Always settle.
      scheduleBandSettle();
      return;
    }
    scheduleBandSettle();
  };

  const onHardenTextarea = useEffectEvent(() => {
    hardenMobileTextarea();
  });
  const onBandSettle = useEffectEvent(() => {
    scheduleBandSettle();
  });
  const onCancelSpeechTransport = useEffectEvent(() => {
    cancelSpeechTransport();
  });
  const onTermData = useEffectEvent((data: string) => {
    sendKey(consumeCtrl(data));
  });
  const onBeforeInput = useEffectEvent((event: InputEvent) => {
    onTextareaBeforeInput(event);
  });
  const onInputEvent = useEffectEvent((event: Event) => {
    onTextareaInput(event);
  });
  const onPaste = useEffectEvent((event: ClipboardEvent) => {
    onTextareaPaste(event);
  });
  const onSeedTermSentinel = useEffectEvent(() => {
    seedTermSentinel();
  });
  const onRestorePinnedScroll = useEffectEvent(() => restorePinnedInteractionScroll());

  useEffect(() => {
    const onBlur = () => {
      stopHeldKeyRepeat();
      toolbarRepeatHandledRef.current = false;
    };
    const onVisibility = () => {
      if (document.visibilityState === "hidden") {
        stopHeldKeyRepeat();
        toolbarRepeatHandledRef.current = false;
      }
    };
    window.addEventListener("blur", onBlur);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      stopHeldKeyRepeat();
      toolbarRepeatHandledRef.current = false;
      window.removeEventListener("blur", onBlur);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, []);

  useEffect(() => {
    return mountTaskTerminalSession({
      handle,
      hostElRef,
      interactionElRef,
      spacerElRef,
      termRef,
      connectionRef,
      schedulePostLayoutRef,
      resetResizeDedupeRef,
      jumpToBottomRef,
      linkServiceRef,
      terminalSnapshotRef,
      copyNoticeTimerRef,
      ctrlTimerRef,
      setStatus,
      setStatusDetail,
      setHasUnseenOutput,
      setLinkMenu,
      syncCopyOverlay,
      sendKey,
      termTextarea,
      cancelExpandSettle,
      clearExpandedInert,
      cancelSpeechTransport: onCancelSpeechTransport,
      seedSentinelFromFocus,
      onHardenTextarea,
      onBandSettle,
      onTermData,
      onBeforeInput,
      onInputEvent,
      onPaste,
      onSeedTermSentinel,
      onRestorePinnedScroll,
    });
  }, [handle]);

  return (
    <section
      className={`terminal-panel${expanded ? " is-expanded" : ""}`}
      data-testid="task-terminal-panel"
      aria-label="Task terminal">
      <div
        className="terminal-interaction-wrap"
        data-testid="terminal-interaction-surface"
        ref={interactionElRef}>
        <div className="terminal-host" ref={hostElRef}></div>
        <div className="terminal-scroll-spacer" ref={spacerElRef} aria-hidden="true"></div>
        {hasUnseenOutput ? (
          <button
            type="button"
            className="terminal-new-output"
            onClick={() => jumpToBottomRef.current?.()}>
            New output ↓
          </button>
        ) : null}
      </div>
      {copyNotice ? (
        <p className="terminal-copy-notice" role="status">
          {copyNotice}
        </p>
      ) : null}
      <div className="terminal-corner-actions">
        {copyOverlayText ? (
          <button
            type="button"
            className="terminal-copy-overlay"
            data-testid="terminal-copy-overlay"
            onClick={() => void copySelection()}>
            Copy
          </button>
        ) : null}
        <button
          type="button"
          className={`terminal-expand-corner${expanded ? " is-armed" : ""}`}
          aria-label="Expand terminal"
          aria-pressed={expanded}
          onPointerDown={(event) => event.preventDefault()}
          onClick={() => toggleExpanded()}>
          ⛶
        </button>
      </div>
      <div
        className={`terminal-status${statusVisible ? "" : " is-empty"}`}
        data-testid="terminal-status"
        aria-hidden={statusVisible ? "false" : "true"}>
        {statusVisible ? (
          <>
            <span className="terminal-status-label">{STATUS_LABELS[status]}</span>
            {statusDetail ? (
              <span className="terminal-status-detail">{statusDetail}</span>
            ) : null}
            {showReconnect ? (
              <button
                type="button"
                className="terminal-status-reconnect"
                onClick={() => requestReconnect()}>
                Reconnect
              </button>
            ) : null}
          </>
        ) : null}
      </div>
      {linkMenu ? (
        <FloatingContextMenu
          open
          anchor={{ x: linkMenu.x, y: linkMenu.y }}
          items={[
            {
              id: "open",
              label: "Open",
              onSelect: () => {
                linkServiceRef.current?.onOpen(linkMenu.url);
                setLinkMenu(null);
              },
            },
            {
              id: "copy",
              label: "Copy",
              onSelect: () => {
                const url = linkMenu.url;
                setLinkMenu(null);
                void (async () => {
                  const copied = await linkServiceRef.current?.onCopy(url);
                  if (copied) {
                    if (copyNoticeTimerRef.current) clearTimeout(copyNoticeTimerRef.current);
                    setCopyNotice("Copied");
                    copyNoticeTimerRef.current = setTimeout(() => {
                      setCopyNotice("");
                      copyNoticeTimerRef.current = undefined;
                    }, 1500);
                    return;
                  }
                  setCopyFallbackText(url);
                  setCopyFallbackOpen(true);
                })();
              },
            },
          ]}
          onClose={() => setLinkMenu(null)}
          ariaLabel="Terminal link actions"
        />
      ) : null}
      {copyFallbackOpen ? (
        <div className="terminal-paste-fallback">
          <p className="terminal-paste-notice" role="status">
            Clipboard unavailable — copy below.
          </p>
          <textarea
            className="terminal-paste-input"
            readOnly
            aria-label="Copy text"
            value={copyFallbackText}></textarea>
          <div className="terminal-paste-actions">
            <button type="button" className="terminal-key" onClick={() => dismissCopyFallback()}>
              Done
            </button>
          </div>
        </div>
      ) : null}
      {pasteFallbackOpen ? (
        <div className="terminal-paste-fallback">
          <p className="terminal-paste-notice" role="status">
            {pasteNotice}
          </p>
          <textarea
            className="terminal-paste-input"
            aria-label="Paste text"
            value={pasteFallbackText}
            onChange={(event) => setPasteFallbackText(event.target.value)}></textarea>
          <div className="terminal-paste-actions">
            <button type="button" className="terminal-key" onClick={() => sendPasteFallback()}>
              Send
            </button>
            <button type="button" className="terminal-key" onClick={() => cancelPasteFallback()}>
              Cancel
            </button>
          </div>
        </div>
      ) : null}
      <div role="status" className="terminal-speech-status">
        {speechModel.state === "connecting" ? <span>Connecting…</span> : null}
        {speechModel.state === "listening" ? <span>Listening</span> : null}
        {speechModel.state === "finalizing" ? <span>Finalizing…</span> : null}
        {speechModel.state === "pause_pending" && pauseCountdownSeconds !== undefined ? (
          <>
            <span>Pausing in {pauseCountdownSeconds}…</span>
            <span>Speak to continue</span>
          </>
        ) : null}
        {speechModel.state === "error" && speechModel.errorMessage ? (
          <span>{speechModel.errorMessage}</span>
        ) : null}
        {speechModel.state !== "error" && speechModel.errorMessage ? (
          <span>{speechModel.errorMessage}</span>
        ) : null}
      </div>
      {speechModel.state !== "idle" ? (
        <div className="terminal-speech-actions">
          <button
            type="button"
            className="terminal-key"
            aria-label="Cancel voice input"
            onPointerDown={onToolbarPointerDown}
            onClick={(event) => {
              const ownedFocus = consumeToolbarPointerOwnedFocus(event);
              cancelSpeechInput();
              refocusTermIfOwned(ownedFocus);
            }}>
            Cancel voice input
          </button>
        </div>
      ) : null}
      <div data-testid="terminal-bottom-controls">
        <div className="terminal-keys" role="toolbar" aria-label="Terminal keys">
          {CONTROL_KEYS.map((key) => {
            const repeatable = isRepeatableKey(key.data);
            return (
              <button
                key={key.label}
                type="button"
                className="terminal-key"
                aria-label={key.ariaLabel}
                onPointerDown={(event) => {
                  if (repeatable) {
                    onRepeatableKeyPointerDown(event, key.data);
                    return;
                  }
                  onToolbarPointerDown(event);
                }}
                onPointerUp={repeatable ? onRepeatableKeyPointerEnd : undefined}
                onPointerCancel={repeatable ? onRepeatableKeyPointerEnd : undefined}
                onLostPointerCapture={repeatable ? onRepeatableKeyPointerEnd : undefined}
                onClick={(event) => onControlKeyClick(event, key.data, repeatable)}>
                {key.label}
              </button>
            );
          })}
          <button
            type="button"
            className={`terminal-key${ctrlArmed ? " is-armed" : ""}`}
            aria-label="Control modifier"
            aria-pressed={ctrlArmed}
            onPointerDown={onToolbarPointerDown}
            onClick={(event) => {
              const ownedFocus = consumeToolbarPointerOwnedFocus(event);
              toggleCtrl();
              refocusTermIfOwned(ownedFocus);
            }}>
            Ctrl
            {ctrlArmed ? (
              <span className="terminal-key-armed-dot" aria-hidden="true"></span>
            ) : null}
          </button>
          <button
            type="button"
            className="terminal-key"
            aria-label="Paste"
            onPointerDown={onToolbarPointerDown}
            onClick={(event) => {
              const ownedFocus = consumeToolbarPointerOwnedFocus(event);
              void requestPaste(ownedFocus);
            }}>
            Paste
          </button>
          <button
            key={BACKSPACE_KEY.label}
            type="button"
            className="terminal-key"
            aria-label={BACKSPACE_KEY.ariaLabel}
            onPointerDown={(event) => onRepeatableKeyPointerDown(event, BACKSPACE_KEY.data)}
            onPointerUp={onRepeatableKeyPointerEnd}
            onPointerCancel={onRepeatableKeyPointerEnd}
            onLostPointerCapture={onRepeatableKeyPointerEnd}
            onClick={(event) =>
              onControlKeyClick(event, BACKSPACE_KEY.data, isRepeatableKey(BACKSPACE_KEY.data))
            }>
            {BACKSPACE_KEY.label}
          </button>
          <button
            type="button"
            className={`terminal-key${micArmed ? " is-armed" : ""}`}
            aria-label={micArmed ? "Stop voice input" : micAriaLabel}
            title={micArmed ? "Stop voice input" : micAriaLabel}
            disabled={
              speechModel.state === "connecting" || speechModel.state === "finalizing"
            }
            onPointerDown={onToolbarPointerDown}
            onClick={(event) => {
              const ownedFocus = consumeToolbarPointerOwnedFocus(event);
              toggleMic();
              refocusTermIfOwned(ownedFocus);
            }}>
            Mic
          </button>
        </div>
      </div>
    </section>
  );
}
