<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { ZerolagInputAddon } from "xterm-zerolag-input";
  import { openTaskTerminalSocket } from "../api";
  import { isKeyboardOpen } from "../viewport";
  import { flingFrames, wheelNotchesFromDrag } from "../terminalTouchScroll";
  import {
    clampPan,
    flooredCols,
    pinchFontSize,
    MAX_FONT_SIZE,
    MIN_FONT_SIZE,
    MIN_TERMINAL_COLS,
  } from "../terminalGeometry";
  import "@xterm/xterm/css/xterm.css";

  interface Props {
    handle: string;
  }

  let { handle }: Props = $props();

  let container: HTMLDivElement | undefined = $state();
  // A dead socket always auto-recovers (scheduleReconnect), so there is no
  // terminal "disconnected" state — only the reconnecting one.
  let status = $state<"connecting" | "connected" | "reconnecting">("connecting");
  let statusDetail = $state("");
  let ctrlArmed = $state(false);
  let hasUnseenOutput = $state(false);
  let expanded = $state(false);

  // Expanded mode hands the terminal the whole screen: on mobile the class
  // hides the task chrome (header/status/actions/details, see styles.css); on
  // desktop it lifts the panel into a fixed full-viewport overlay. The class
  // lives on <html> so page-level chrome outside this component can react.
  const EXPANDED_CLASS = "terminal-expanded";
  const setExpanded = (next: boolean) => {
    expanded = next;
    document.documentElement.classList.toggle(EXPANDED_CLASS, next);
  };

  // Assigned inside onMount so the key bar can reach the live socket/terminal.
  let sendKey: (data: string) => void = () => {};
  let focusTerm: () => void = () => {};
  let jumpToBottom: () => void = () => {};
  let requestReconnect: () => void = () => {};
  let requestPaste: () => void = () => {};
  let blurTerm: () => void = () => {};
  let refitAfterLayout: () => void = () => {};

  const STATUS_LABELS: Record<typeof status, string> = {
    connecting: "Connecting…",
    connected: "Connected",
    reconnecting: "Reconnecting…",
  };

  // Control-key bar: keys the iOS keyboard lacks. The "Ctrl" button is a sticky
  // modifier folded into the next key — from the keyboard OR this bar — by
  // consumeCtrl below. It auto-expires so a forgotten arm can't silently
  // corrupt a later keystroke.
  const CONTROL_KEYS = [
    { label: "Esc", data: "\x1b" },
    { label: "Tab", data: "\t" },
    { label: "⌃C", data: "\x03" },
    { label: "←", data: "\x1b[D" },
    { label: "↑", data: "\x1b[A" },
    { label: "↓", data: "\x1b[B" },
    { label: "→", data: "\x1b[C" },
  ];

  // A sticky modifier the user forgot they armed would mangle the next thing
  // they type minutes later, so it auto-disarms after a short window.
  const CTRL_ARM_TIMEOUT_MS = 4000;
  let ctrlTimer: ReturnType<typeof setTimeout> | undefined;

  const disarmCtrl = () => {
    ctrlArmed = false;
    if (ctrlTimer) {
      clearTimeout(ctrlTimer);
      ctrlTimer = undefined;
    }
  };

  const toggleCtrl = () => {
    if (ctrlArmed) {
      disarmCtrl();
      return;
    }
    ctrlArmed = true;
    if (ctrlTimer) clearTimeout(ctrlTimer);
    ctrlTimer = setTimeout(disarmCtrl, CTRL_ARM_TIMEOUT_MS);
  };

  // Fold Ctrl into a key: letters become their control code, cursor keys become
  // the CSI form with the Ctrl modifier (param 5); anything else passes through.
  const controlModify = (data: string): string => {
    if (data.length === 1) {
      const code = data.toLowerCase().charCodeAt(0);
      if (code >= 97 && code <= 122) return String.fromCharCode(code - 96);
    }
    const cursor = /^\x1b\[([ABCD])$/.exec(data);
    if (cursor) return `\x1b[1;5${cursor[1]}`;
    return data;
  };

  // Apply an armed Ctrl to the next key from either input surface, consuming
  // the arm so it never affects a second key.
  const consumeCtrl = (data: string): string => {
    if (!ctrlArmed) return data;
    disarmCtrl();
    return controlModify(data);
  };

  // The 80-column PTY floor means the font is no longer a column-count lever
  // (narrow viewports pan instead of wrapping), so every viewport — phone,
  // landscape phone, desktop — gets the same comfortable cell.
  // Pinch-to-zoom persists any override.
  const DEFAULT_FONT_SIZE = 13;

  // Pinch-to-zoom persists the operator's legibility choice; a valid stored
  // size wins over the per-device default. localStorage can throw (Safari
  // private mode), so reads/writes are best-effort.
  const FONT_SIZE_STORAGE_KEY = "ajax.terminal.fontSize";
  const persistedFontSize = (): number | undefined => {
    try {
      const raw = window.localStorage.getItem(FONT_SIZE_STORAGE_KEY);
      if (!raw) return undefined;
      const parsed = Number.parseInt(raw, 10);
      if (!Number.isFinite(parsed) || parsed < MIN_FONT_SIZE || parsed > MAX_FONT_SIZE) {
        return undefined;
      }
      return parsed;
    } catch {
      return undefined;
    }
  };
  const persistFontSize = (size: number) => {
    try {
      window.localStorage.setItem(FONT_SIZE_STORAGE_KEY, String(size));
    } catch {
      // Best-effort: the session still uses the new size.
    }
  };

  onMount(() => {
    const term = new Terminal({
      cursorBlink: true,
      fontFamily: "ui-monospace, SF Mono, Menlo, monospace",
      fontSize: persistedFontSize() ?? DEFAULT_FONT_SIZE,
      theme: {
        background: "#1c1714",
        foreground: "#f4eee0",
        cursor: "#52a095",
      },
    });
    const fitAddon = new FitAddon();
    const zerolag = new ZerolagInputAddon();
    const outputDecoder = new TextDecoder();
    term.loadAddon(fitAddon);
    term.loadAddon(zerolag);

    if (container) {
      term.open(container);
      // Harden the hidden textarea xterm owns so mobile keyboards don't corrupt
      // terminal input with autocorrect/autocapitalize.
      const input = term.textarea;
      if (input) {
        input.setAttribute("autocapitalize", "off");
        input.setAttribute("autocorrect", "off");
        input.setAttribute("autocomplete", "off");
        input.setAttribute("spellcheck", "false");
      }
      // Defer the first fit until layout settles so the PTY gets real dimensions.
      requestAnimationFrame(() => fitAddon.fit());
    }

    // Auto-follow new output only while the user is at the bottom of the
    // scrollback. A tmux-attached session redraws constantly (status bar,
    // idle prompt refresh), and unconditionally calling scrollToBottom() on
    // every output frame yanked the view back down the instant a user tried
    // to scroll up — scrolling looked completely broken.
    let pinnedToBottom = true;
    term.onScroll(() => {
      pinnedToBottom = term.buffer.active.viewportY >= term.buffer.active.baseY;
      if (pinnedToBottom) hasUnseenOutput = false;
    });

    // Wheel/touch scrolling always uses Ajax-owned xterm scrollback. Capture
    // before xterm layers can handle the gesture, and never forward scroll
    // intent into tmux or the foreground terminal app.
    const TOUCH_SCROLL_THRESHOLD_PX = 6;
    let touchActive = false;
    let touchLastY = 0;
    let touchAccumPx = 0;
    let touchLastX = 0;
    let touchAccumXPx = 0;

    // Two-finger pinch adjusts the font size (the legibility ↔ visible-columns
    // lever now that the PTY keeps an 80-column floor). The gesture scales
    // from the size it started at, so a slow pinch can't compound drift.
    let pinchStartDistance = 0;
    let pinchBaseFontSize = 0;

    const touchDistance = (touches: TouchList): number =>
      Math.hypot(
        touches[0].clientX - touches[1].clientX,
        touches[0].clientY - touches[1].clientY,
      );

    const cellHeightPx = (): number => {
      const viewport = container?.querySelector<HTMLElement>(".xterm-viewport");
      const height = viewport?.clientHeight ?? 0;
      // jsdom and pre-layout paints report 0; fall back to a sane line height.
      return height > 0 && term.rows > 0 ? height / term.rows : 18;
    };

    const scrollLocalLines = (lines: number) => {
      const step = lines > 0 ? 1 : -1;
      for (let i = 0; i < Math.abs(lines); i += 1) {
        term.scrollLines(step);
      }
    };

    // Momentum: a fast release keeps scrolling with decaying inertia (the
    // synthetic scroll otherwise stops dead the instant the finger lifts).
    // The frame sequence is precomputed by flingFrames; any new touch or
    // wheel cancels it so the user always wins.
    let flingHandle = 0;
    let flingVelocity = 0; // px per ms, positive = toward newest output
    let lastMoveTime = 0;
    let touchScrolled = false;

    const cancelFling = () => {
      if (flingHandle) {
        cancelAnimationFrame(flingHandle);
        flingHandle = 0;
      }
    };

    const startFling = (frames: number[]) => {
      cancelFling();
      let index = 0;
      const step = () => {
        if (index >= frames.length) {
          flingHandle = 0;
          return;
        }
        const lines = frames[index];
        index += 1;
        if (lines !== 0) scrollLocalLines(lines);
        flingHandle = requestAnimationFrame(step);
      };
      flingHandle = requestAnimationFrame(step);
    };

    const onTouchStart = (event: TouchEvent) => {
      cancelFling();
      if (event.touches.length === 2) {
        touchActive = false;
        pinchStartDistance = touchDistance(event.touches);
        pinchBaseFontSize = term.options.fontSize ?? DEFAULT_FONT_SIZE;
        return;
      }
      pinchStartDistance = 0;
      if (event.touches.length !== 1) {
        touchActive = false;
        return;
      }
      touchActive = true;
      touchScrolled = false;
      touchAccumPx = 0;
      touchAccumXPx = 0;
      touchLastY = event.touches[0].clientY;
      touchLastX = event.touches[0].clientX;
      flingVelocity = 0;
      lastMoveTime = performance.now();
    };

    const onTouchMove = (event: TouchEvent) => {
      if (event.touches.length === 2 && pinchStartDistance > 0) {
        // Own the pinch so iOS can't page-zoom; font rounding means the
        // terminal only re-renders when the size crosses a whole pixel.
        if (event.cancelable) event.preventDefault();
        const next = pinchFontSize(
          pinchBaseFontSize,
          pinchStartDistance,
          touchDistance(event.touches),
        );
        if (next !== term.options.fontSize) {
          term.options.fontSize = next;
          persistFontSize(next);
          scheduleDebouncedRefit();
        }
        return;
      }
      if (!touchActive || event.touches.length !== 1) return;
      const touch = event.touches[0];
      const dy = touchLastY - touch.clientY;
      touchAccumPx += dy;
      touchAccumXPx += touchLastX - touch.clientX;
      touchLastY = touch.clientY;
      touchLastX = touch.clientX;

      // Release-velocity estimate for the momentum fling; low-passed so one
      // jittery event can't spike it.
      const now = performance.now();
      const dtMs = Math.max(1, now - lastMoveTime);
      lastMoveTime = now;
      flingVelocity = 0.8 * (dy / dtMs) + 0.2 * flingVelocity;

      if (
        Math.abs(touchAccumPx) < TOUCH_SCROLL_THRESHOLD_PX &&
        Math.abs(touchAccumXPx) < TOUCH_SCROLL_THRESHOLD_PX
      ) {
        return;
      }
      touchScrolled = true;

      // Past the threshold this is a scroll, not a tap, so own the gesture NOW —
      // before a full cell of movement accumulates. iOS Safari latches native
      // momentum scrolling in the first pixels of a drag and can't be cancelled
      // later; preventing default here (rather than only once a whole notch
      // lands) stops that native scroll from racing our scrollLines() and stops
      // iOS from synthesizing the click that would pop the keyboard.
      if (event.cancelable) event.preventDefault();

      // Horizontal component pans the 80-col canvas within the host; the host
      // is overflow:hidden so only this handler ever moves it.
      if (touchAccumXPx !== 0 && container) {
        container.scrollLeft = clampPan(
          container.scrollLeft + touchAccumXPx,
          container.scrollWidth,
          container.clientWidth,
        );
        touchAccumXPx = 0;
      }

      const { notches, remainderPx } = wheelNotchesFromDrag(touchAccumPx, cellHeightPx());
      touchAccumPx = remainderPx;
      if (notches === 0) return;
      scrollLocalLines(notches);
    };

    const resetTouchState = () => {
      flingVelocity = 0;
      touchActive = false;
      touchAccumPx = 0;
      touchAccumXPx = 0;
      pinchStartDistance = 0;
    };

    const onTouchEnd = () => {
      // Only a gesture that actually scrolled may fling; a tap with a few
      // pixels of jitter must stay a tap.
      if (touchActive && touchScrolled) {
        const frames = flingFrames(flingVelocity, cellHeightPx());
        if (frames.length) startFling(frames);
      }
      resetTouchState();
    };

    // touchcancel means the system stole the gesture (e.g. an incoming call
    // sheet); momentum from a stolen gesture would feel haunted, so reset
    // without flinging.
    const onTouchCancel = resetTouchState;

    const onWheel = (event: WheelEvent) => {
      cancelFling();
      const lineDelta =
        event.deltaMode === WheelEvent.DOM_DELTA_PIXEL
          ? Math.trunc(event.deltaY / cellHeightPx())
          : Math.trunc(event.deltaY);
      if (lineDelta === 0) return;

      if (event.cancelable) event.preventDefault();
      scrollLocalLines(lineDelta);
    };

    const touchStartOptions: AddEventListenerOptions = { passive: true, capture: true };
    const touchMoveOptions: AddEventListenerOptions = { passive: false, capture: true };
    const scrollEndOptions: AddEventListenerOptions = { passive: true, capture: true };
    const wheelOptions: AddEventListenerOptions = { passive: false, capture: true };

    container?.addEventListener("touchstart", onTouchStart, touchStartOptions);
    container?.addEventListener("touchmove", onTouchMove, touchMoveOptions);
    container?.addEventListener("touchend", onTouchEnd, scrollEndOptions);
    container?.addEventListener("touchcancel", onTouchCancel, scrollEndOptions);
    container?.addEventListener("wheel", onWheel, wheelOptions);

    // Reassigned on every (re)connect; the input/resize closures below read this
    // binding live, so they always target the current socket.
    let socket: WebSocket;

    // Mobile Safari drops the WebSocket whenever it backgrounds the tab, and a
    // failed tmux attach closes it too. A dead socket must recover on its own
    // (capped backoff) and immediately on foreground, rather than stranding the
    // user on a frozen pane with the word "closed".
    let reconnectAttempts = 0;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    let disposed = false;
    const RECONNECT_MAX_DELAY_MS = 15000;

    // The iOS keyboard animates the visual viewport shorter over several frames.
    // A tmux-attached client SIGWINCHes the shared window on every resize, so
    // spraying resize frames during that animation corrupts the pane. Withhold
    // server resizes while the keyboard is open (isKeyboardOpen — the same
    // hysteresis-guarded state that drives the CSS takeover, so the freeze and
    // the chrome collapse can never disagree); the local fit still runs so
    // xterm stays visually correct, and a single resize is flushed once the
    // viewport settles.
    const sendResize = () => {
      if (socket.readyState !== WebSocket.OPEN) return;
      if (isKeyboardOpen()) return;
      socket.send(
        JSON.stringify({
          type: "resize",
          cols: term.cols,
          rows: term.rows,
        }),
      );
    };

    sendKey = (data: string) => {
      if (socket.readyState !== WebSocket.OPEN) return;
      socket.send(JSON.stringify({ type: "input", data }));
    };
    focusTerm = () => term.focus();
    // iPhone keyboards can't dismiss themselves and the keyboard-open chrome
    // collapse hides the Back button; blurring xterm's textarea is the only
    // way to hand the screen back to the full-height terminal.
    blurTerm = () => term.blur();
    jumpToBottom = () => {
      term.scrollToBottom();
      hasUnseenOutput = false;
      term.focus();
    };
    // iOS long-press paste doesn't reliably reach xterm's hidden textarea, so
    // the key bar offers an explicit Paste. term.paste() honors bracketed-paste
    // mode and flows through the normal onData → socket path. Failures must be
    // visible: silently doing nothing reads as a broken button.
    requestPaste = () => {
      const clipboard = navigator.clipboard;
      if (!clipboard || typeof clipboard.readText !== "function") {
        statusDetail = "Clipboard unavailable in this browser";
        return;
      }
      clipboard
        .readText()
        .then((text) => {
          if (text) term.paste(text);
          statusDetail = "";
          term.focus();
        })
        .catch(() => {
          statusDetail = "Clipboard read failed — allow paste access and retry";
        });
    };

    let refitFrame = 0;
    let viewportResizeTimer: ReturnType<typeof setTimeout> | undefined;

    // Fit rows to the container but never let the PTY drop below 80 columns:
    // the hosted tmux/Claude Code TUI assumes ~80, and a narrower PTY wraps
    // nearly every line. When the floor exceeds what fits, the canvas extends
    // past the right edge and horizontal pan brings it into view.
    const fitNow = () => {
      if (isKeyboardOpen()) {
        // The server resize is withheld while the keyboard is open, so the
        // local grid must not change either: a grid smaller than the PTY makes
        // tmux cursor-address rows that no longer exist locally, and xterm
        // clamps those writes to its bottom row — the TUI input box drifts up
        // and overwrites the line below it. Keep grid == PTY and crop the
        // taller canvas bottom-anchored so the cursor/input row stays visible
        // above the keyboard.
        if (container) {
          container.scrollTop = Math.max(0, container.scrollHeight - container.clientHeight);
        }
        if (pinnedToBottom) term.scrollToBottom();
        return;
      }
      if (container) container.scrollTop = 0;
      const proposed = fitAddon.proposeDimensions();
      if (proposed && Number.isFinite(proposed.rows) && proposed.rows > 0) {
        term.resize(flooredCols(proposed.cols, MIN_TERMINAL_COLS), proposed.rows);
      } else {
        // jsdom / pre-layout paints propose nothing; plain fit is the best guess.
        fitAddon.fit();
      }
      if (pinnedToBottom) term.scrollToBottom();
    };

    // Connection-time refit: the PTY must learn the real size immediately (the
    // keyboard is never open at connect), so send without debouncing.
    const scheduleImmediateRefit = () => {
      if (refitFrame) cancelAnimationFrame(refitFrame);
      refitFrame = requestAnimationFrame(() => {
        fitNow();
        sendResize();
      });
    };

    // Event-driven refit (container/window/orientation/keyboard): fit locally
    // right away, but coalesce the server resize behind a debounce so a burst —
    // e.g. the keyboard animation — collapses into a single frame after things
    // settle (and is dropped entirely while the keyboard is open).
    const scheduleDebouncedRefit = () => {
      if (refitFrame) cancelAnimationFrame(refitFrame);
      refitFrame = requestAnimationFrame(fitNow);

      if (viewportResizeTimer) clearTimeout(viewportResizeTimer);
      viewportResizeTimer = setTimeout(() => {
        sendResize();
        viewportResizeTimer = undefined;
      }, 300);
    };

    const schedulePostLayoutRefit = () => {
      scheduleImmediateRefit();
      requestAnimationFrame(scheduleImmediateRefit);
    };
    // Discrete layout jumps (the ⛶ expand toggle) refit through the immediate
    // path: waiting out the debounce leaves the grid misfit in the new space
    // for a visible beat.
    refitAfterLayout = schedulePostLayoutRefit;

    const resizeObserver =
      typeof ResizeObserver !== "undefined" ? new ResizeObserver(scheduleDebouncedRefit) : null;
    if (container && resizeObserver) {
      resizeObserver.observe(container);
    }

    window.addEventListener("resize", scheduleDebouncedRefit);
    window.addEventListener("orientationchange", scheduleDebouncedRefit);
    const viewport = window.visualViewport;
    viewport?.addEventListener("resize", scheduleDebouncedRefit);
    viewport?.addEventListener("scroll", scheduleDebouncedRefit);

    const readMessageData = async (data: unknown): Promise<string> => {
      if (typeof data === "string") return data;
      if (data instanceof Blob) {
        // Every supported browser has Blob.text(), but jsdom's Blob does not —
        // the FileReader fallback is what the component tests exercise.
        if ("text" in data && typeof data.text === "function") return data.text();
        return new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.addEventListener("load", () => resolve(String(reader.result ?? "")));
          reader.addEventListener("error", () => reject(reader.error));
          reader.readAsText(data);
        });
      }
      if (data instanceof ArrayBuffer) return new TextDecoder().decode(data);
      return String(data);
    };

    const writeOutput = (text: string) => {
      term.write(text);
      if (pinnedToBottom) {
        term.scrollToBottom();
      } else {
        hasUnseenOutput = true;
      }
      zerolag.clearFlushed();
      zerolag.rerender();
    };

    const onSocketMessage = async (event: MessageEvent) => {
      const data = await readMessageData(event.data);
      try {
        const payload = JSON.parse(data) as { type?: string; data?: string };
        if (payload.type === "output" && payload.data) {
          const binary = atob(payload.data);
          const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
          writeOutput(outputDecoder.decode(bytes, { stream: true }));
        } else if (payload.type === "error" && "error" in payload && payload.error) {
          statusDetail = String(payload.error);
        }
      } catch {
        writeOutput(data);
      }
    };

    const scheduleReconnect = () => {
      if (disposed) return;
      status = "reconnecting";
      const delay = Math.min(RECONNECT_MAX_DELAY_MS, 1000 * 2 ** reconnectAttempts);
      reconnectAttempts += 1;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = setTimeout(() => {
        if (!disposed) connect();
      }, delay);
    };

    const reconnectNow = () => {
      if (disposed) return;
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = undefined;
      }
      reconnectAttempts = 0;
      status = "connecting";
      connect();
    };

    function connect() {
      socket = openTaskTerminalSocket(handle);
      socket.addEventListener("open", () => {
        // A successful open resets the backoff. A fresh tmux attach repaints the
        // pane and the resize-on-open makes tmux redraw at the real size, so no
        // explicit refresh frame is needed on reconnect.
        const isReconnect = reconnectAttempts > 0;
        reconnectAttempts = 0;
        statusDetail = "";
        status = "connected";
        // Keystrokes sent on the previous socket will never be echoed by this
        // new one, so drop the local input overlay; otherwise those characters
        // would linger as ghost text until the next output frame reconciled it.
        if (isReconnect) zerolag.clear();
        schedulePostLayoutRefit();
        requestAnimationFrame(() => term.focus());
      });
      socket.addEventListener("message", onSocketMessage);
      // An error is followed by close; let the close handler own reconnect so we
      // never schedule it twice.
      socket.addEventListener("error", () => {});
      socket.addEventListener("close", () => {
        if (disposed) return;
        scheduleReconnect();
      });
    }

    // Reconnect immediately when the tab returns to the foreground instead of
    // waiting out the backoff (mobile Safari kills the socket on background).
    const onVisibility = () => {
      if (
        document.visibilityState === "visible" &&
        status === "reconnecting"
      ) {
        reconnectNow();
      }
    };
    document.addEventListener("visibilitychange", onVisibility);

    // Exposed to the status banner's manual "Reconnect" button.
    requestReconnect = reconnectNow;

    connect();

    term.onData((raw) => {
      if (socket.readyState !== WebSocket.OPEN) return;

      // Sticky Ctrl folds into this key (letter → control code, cursor key →
      // Ctrl-modified CSI). The folded byte then takes the normal branches, so
      // keys Ctrl leaves untouched (Enter, backspace) keep their overlay
      // bookkeeping instead of slipping past it.
      const data = consumeCtrl(raw);

      if (data === "\r") {
        zerolag.clear();
        socket.send(JSON.stringify({ type: "input", data }));
        return;
      }

      if (data === "\x7f") {
        // Raw tmux/TUI attach: every keystroke is sent immediately, so the
        // remote PTY owns line editing. removeChar() only keeps the latency
        // overlay in sync — the backspace itself must always reach the PTY,
        // otherwise the deleted character lives on in the real buffer.
        zerolag.removeChar();
        socket.send(JSON.stringify({ type: "input", data }));
        return;
      }

      // Printable keystrokes are sent to the PTY immediately (raw attach), so
      // the zero-lag overlay tracks them as "flushed" — in flight, echo not yet
      // received — rather than "pending" (locally held, unsent). The output
      // handler's clearFlushed() wipes them the moment the echo lands, so the
      // overlay never accumulates text that already lives in the real buffer.
      if (data.length === 1 && data.charCodeAt(0) >= 32) {
        const { count, text } = zerolag.getFlushed();
        zerolag.setFlushed(count + 1, text + data);
      }

      socket.send(JSON.stringify({ type: "input", data }));
    });

    return () => {
      disposed = true;
      setExpanded(false);
      if (refitFrame) cancelAnimationFrame(refitFrame);
      if (viewportResizeTimer) clearTimeout(viewportResizeTimer);
      if (ctrlTimer) clearTimeout(ctrlTimer);
      if (reconnectTimer) clearTimeout(reconnectTimer);
      document.removeEventListener("visibilitychange", onVisibility);
      resizeObserver?.disconnect();
      window.removeEventListener("resize", scheduleDebouncedRefit);
      window.removeEventListener("orientationchange", scheduleDebouncedRefit);
      viewport?.removeEventListener("resize", scheduleDebouncedRefit);
      viewport?.removeEventListener("scroll", scheduleDebouncedRefit);
      cancelFling();
      container?.removeEventListener("touchstart", onTouchStart, touchStartOptions);
      container?.removeEventListener("touchmove", onTouchMove, touchMoveOptions);
      container?.removeEventListener("touchend", onTouchEnd, scrollEndOptions);
      container?.removeEventListener("touchcancel", onTouchCancel, scrollEndOptions);
      container?.removeEventListener("wheel", onWheel, wheelOptions);
      socket.close();
      zerolag.dispose();
      term.dispose();
    };
  });
</script>

<section class="terminal-panel" data-testid="task-terminal-panel" aria-label="Task terminal">
  <div class="terminal-host task-terminal-viewport" bind:this={container}></div>
  {#if hasUnseenOutput}
    <button
      type="button"
      class="terminal-new-output"
      onclick={() => {
        jumpToBottom();
      }}>New output ↓</button>
  {/if}
  <div class="terminal-keys" role="toolbar" aria-label="Terminal keys">
    {#each CONTROL_KEYS as key (key.label)}
      <button
        type="button"
        class="terminal-key"
        onmousedown={(event) => event.preventDefault()}
        onclick={() => {
          sendKey(consumeCtrl(key.data));
          focusTerm();
        }}>{key.label}</button>
    {/each}
    <button
      type="button"
      class="terminal-key"
      class:is-armed={ctrlArmed}
      aria-pressed={ctrlArmed}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => {
        toggleCtrl();
        focusTerm();
      }}>Ctrl{#if ctrlArmed}<span class="terminal-key-armed-dot" aria-hidden="true"></span>{/if}</button>
    <button
      type="button"
      class="terminal-key"
      onmousedown={(event) => event.preventDefault()}
      onclick={() => requestPaste()}>Paste</button>
    <button
      type="button"
      class="terminal-key"
      aria-label="Hide keyboard"
      onclick={() => blurTerm()}>⌄</button>
    <button
      type="button"
      class="terminal-key terminal-key-expand"
      class:is-armed={expanded}
      aria-label="Expand terminal"
      aria-pressed={expanded}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => {
        setExpanded(!expanded);
        refitAfterLayout();
      }}>⛶</button>
  </div>
  {#if status !== "connected" || statusDetail}
    <div class="terminal-status" data-testid="terminal-status">
      <span class="terminal-status-label">{STATUS_LABELS[status]}</span>
      {#if statusDetail}
        <span class="terminal-status-detail">{statusDetail}</span>
      {/if}
      {#if status === "reconnecting"}
        <button
          type="button"
          class="terminal-status-reconnect"
          onclick={() => requestReconnect()}>Reconnect</button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .terminal-panel {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    margin-top: 16px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: var(--paper);
    overflow: hidden;
  }

  /* A landscape phone exceeds the width breakpoint but must not get the
     fixed desktop panel height — its takeover layout flex-fills instead. */
  @media (min-width: 768px) and (not ((pointer: coarse) and (max-height: 500px))) {
    .terminal-panel {
      height: min(58vh, 560px);
    }
  }

  @media (max-width: 767px), (pointer: coarse) and (max-height: 500px) {
    .terminal-panel {
      margin-top: 8px;
    }

    .terminal-host {
      padding: 4px;
    }

    .terminal-keys {
      gap: 4px;
      padding: 3px 4px;
    }

    .terminal-key {
      min-height: 32px;
      padding: 2px 8px;
      font-size: 12px;
    }
  }

  .terminal-host {
    flex: 1 1 auto;
    min-height: 0;
    padding: 8px;
    /* The 80-column floor can make the xterm canvas wider than the phone
       viewport. The host clips it and the touch handler pans it via
       scrollLeft (programmatic scrolling works on overflow:hidden boxes);
       nothing else may scroll this element. */
    overflow: hidden;
    /* Ajax synthesizes 100% of scrolling from touch drags via
       term.scrollLines() (see the touchmove handler). Without touch-action:none
       iOS Safari latches a native pan in the first pixels of a vertical drag —
       before the handler's threshold fires preventDefault() — then delivers the
       rest of the gesture as non-cancelable touchmoves. That native pan (which
       has nothing to move: every scroll container is overflow:hidden) races and
       beats scrollLines(), so the terminal appears completely unscrollable on
       touch. none keeps every touchmove cancelable and hands the whole gesture
       to our handler. */
    touch-action: none;
  }

  .terminal-new-output {
    align-self: center;
    min-height: 36px;
    margin: 0 8px 6px;
    padding: 6px 12px;
    border: 1px solid var(--teal);
    border-radius: var(--radius-sm);
    background: var(--teal-deep);
    color: var(--paper);
    font-size: 12px;
    font-weight: 700;
  }

  .terminal-keys {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding: 6px 8px;
    border-top: 1px solid var(--rule);
    background: var(--paper);
  }

  .terminal-key {
    flex: none;
    min-width: 44px;
    min-height: 40px;
    padding: 6px 10px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--ink);
    font-family: var(--mono);
    font-size: 13px;
  }

  .terminal-key.is-armed {
    background: var(--teal-deep);
    border-color: var(--teal);
    color: var(--paper);
    animation: terminal-key-pulse 1s ease-in-out infinite;
  }

  /* Signals the armed modifier is live and temporary (it auto-expires). */
  @keyframes terminal-key-pulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 rgba(82, 160, 149, 0.6);
    }
    50% {
      box-shadow: 0 0 0 3px rgba(82, 160, 149, 0);
    }
  }

  .terminal-key-armed-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-left: 5px;
    border-radius: 50%;
    background: var(--paper);
    vertical-align: middle;
  }

  .terminal-status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-top: 1px solid var(--rule);
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }

  .terminal-status-detail {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-transform: none;
    letter-spacing: normal;
  }

  .terminal-status-reconnect {
    flex: none;
    min-height: 32px;
    margin-left: auto;
    padding: 4px 12px;
    border: 1px solid var(--teal);
    border-radius: var(--radius-sm);
    background: var(--teal-deep);
    color: var(--paper);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  :global(.terminal-panel .xterm) {
    height: 100%;
    min-width: 0;
  }

  :global(.terminal-panel .xterm-viewport) {
    /* Ajax owns 100% of scrollback: every touch and wheel gesture is
       intercepted and translated into term.scrollLines(). Native scrolling here
       is not just redundant — on iOS Safari the momentum-composited layer that
       -webkit-overflow-scrolling: touch created retained stale pixels when
       xterm rewrote row text, so one drag would native-scroll the layer AND
       scrollLines the buffer, desyncing them into duplicated/overwritten/
       unreadable rows. Disable native scrolling entirely so only scrollLines
       moves the view. touch-action:none stops iOS Safari from claiming the
       vertical drag as a native pan before our touchmove handler can. */
    overflow: hidden;
    overscroll-behavior: contain;
    touch-action: none;
  }

  /* No max-width clamps on .xterm-screen/.xterm-rows or the render layers:
     the renderer sizes them to cols × cellWidth, and with the 80-column floor
     they legitimately exceed the host width. The host's overflow:hidden +
     scrollLeft pan owns the clipping instead. */

  /* xterm 6 renders VS Code's 14px DOM scrollbar inside the terminal. On a
     phone it overlaps ~3 columns of text and flickers visible on every tmux
     redraw, while touch scrolling is entirely Ajax-owned (scrollLines) — so
     it is pure noise there. Desktop keeps it: it is proportionate and
     mouse-draggable. */
  @media (pointer: coarse) {
    :global(.terminal-panel .xterm-scrollable-element > .scrollbar) {
      display: none;
    }
  }
</style>
