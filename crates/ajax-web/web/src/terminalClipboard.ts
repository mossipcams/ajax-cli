export const COPY_NOTICE_MS = 2500;

export type ClipboardUiSnapshot = {
  pasteFallbackOpen: boolean;
  copyOverlayOpen: boolean;
  copyFallbackOpen: boolean;
  copyOverlayText: string;
  notice: string;
};

export type TerminalClipboardUi = {
  snapshot(): ClipboardUiSnapshot;
  openPasteFallback(): void;
  closePasteFallback(): void;
  /** Close paste fallback and return trimmed text to paste (may be ""). */
  takePasteFallbackText(raw: string): string;
  dismissCopyUi(): void;
  /** Arm copy overlay with selected text; no-op/dismiss if empty. */
  presentCopySelection(text: string): void;
  /** After overlay Copy tap: hide overlay; caller runs copyText. */
  beginCopyAttempt(): string;
  /** copyText succeeded → flash "Copied", clear text/fallback. */
  noteCopySucceeded(): void;
  /** copyText failed → open copy fallback with current text. */
  noteCopyFailed(): void;
  flashNotice(message: string): void;
  clearNotice(): void;
  dispose(): void;
};

export function createTerminalClipboardUi(options?: {
  noticeMs?: number;
  schedule?: (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
  clearSchedule?: (id: ReturnType<typeof setTimeout>) => void;
  onChange?: (snap: ClipboardUiSnapshot) => void;
}): TerminalClipboardUi {
  const noticeMs = options?.noticeMs ?? COPY_NOTICE_MS;
  const schedule =
    options?.schedule ?? ((fn: () => void, ms: number) => setTimeout(fn, ms));
  const clearSchedule = options?.clearSchedule ?? clearTimeout;
  const onChange = options?.onChange;

  let pasteFallbackOpen = false;
  let copyOverlayOpen = false;
  let copyFallbackOpen = false;
  let copyOverlayText = "";
  let notice = "";
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  const snapshot = (): ClipboardUiSnapshot => ({
    pasteFallbackOpen,
    copyOverlayOpen,
    copyFallbackOpen,
    copyOverlayText,
    notice,
  });

  const emit = () => {
    onChange?.(snapshot());
  };

  const clearNoticeTimer = () => {
    if (noticeTimer !== undefined) {
      clearSchedule(noticeTimer);
      noticeTimer = undefined;
    }
  };

  const setNotice = (message: string, autoClear: boolean) => {
    notice = message;
    clearNoticeTimer();
    if (autoClear && message) {
      noticeTimer = schedule(() => {
        if (disposed) return;
        notice = "";
        noticeTimer = undefined;
        emit();
      }, noticeMs);
    }
    emit();
  };

  return {
    snapshot,

    openPasteFallback() {
      pasteFallbackOpen = true;
      emit();
    },

    closePasteFallback() {
      pasteFallbackOpen = false;
      emit();
    },

    takePasteFallbackText(raw: string) {
      pasteFallbackOpen = false;
      emit();
      return raw.trim();
    },

    dismissCopyUi() {
      copyOverlayOpen = false;
      copyFallbackOpen = false;
      copyOverlayText = "";
      emit();
    },

    presentCopySelection(text: string) {
      if (!text) {
        copyOverlayOpen = false;
        copyFallbackOpen = false;
        copyOverlayText = "";
      } else {
        copyOverlayText = text;
        copyOverlayOpen = true;
        copyFallbackOpen = false;
      }
      emit();
    },

    beginCopyAttempt() {
      const text = copyOverlayText;
      copyOverlayOpen = false;
      emit();
      return text;
    },

    noteCopySucceeded() {
      copyOverlayText = "";
      copyFallbackOpen = false;
      setNotice("Copied", true);
    },

    noteCopyFailed() {
      copyFallbackOpen = true;
      emit();
    },

    flashNotice(message: string) {
      setNotice(message, true);
    },

    clearNotice() {
      setNotice("", false);
    },

    dispose() {
      disposed = true;
      clearNoticeTimer();
    },
  };
}
