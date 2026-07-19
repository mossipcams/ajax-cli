import type { Terminal } from "@xterm/xterm";

export type TerminalScrollSyncDeps = {
  interactionEl: HTMLElement;
  spacerEl: HTMLElement | null;
  getTerminal: () => Terminal | undefined;
  onUnseenOutput: (unseen: boolean) => void;
};

export type TerminalScrollSync = {
  syncSpacer: () => void;
  scrollInteractionToBottom: () => void;
  refreshFollow: () => void;
  applyOutput: () => void;
  onInteractionScroll: () => void;
  onTermScroll: () => void;
  setFollowLive: (v: boolean) => void;
  setSyncingScroll: (v: boolean) => void;
};

export function createTerminalScrollSync(deps: TerminalScrollSyncDeps): TerminalScrollSync {
  const { interactionEl, spacerEl, getTerminal, onUnseenOutput } = deps;

  let followLive = true;
  let syncingScroll = false;
  let wrapperDroveScroll = false;

  const cellHeightPx = () => {
    const term = getTerminal();
    if (!term || !interactionEl || term.rows <= 0) return 18;
    return Math.max(1, interactionEl.clientHeight / term.rows);
  };

  const scrollbackLines = () => {
    const term = getTerminal();
    if (!term) return 0;
    return Math.max(0, term.buffer.active.length - term.rows);
  };

  const viewportTopLine = () => getTerminal()?.buffer.active.viewportY ?? 0;

  const syncSpacer = () => {
    const term = getTerminal();
    if (!term || !spacerEl || !interactionEl) return;
    spacerEl.style.height = `${scrollbackLines() * cellHeightPx()}px`;
  };

  const scrollInteractionToBottom = () => {
    if (!interactionEl) return;
    interactionEl.scrollTop = Math.max(0, interactionEl.scrollHeight - interactionEl.clientHeight);
  };

  const refreshFollow = () => {
    if (!interactionEl) return;
    const atBottom =
      interactionEl.scrollHeight <= interactionEl.clientHeight + 1 ||
      interactionEl.scrollTop + interactionEl.clientHeight >= interactionEl.scrollHeight - 1;
    followLive = atBottom;
    if (atBottom) onUnseenOutput(false);
  };

  const syncWrapperFromTerm = () => {
    const term = getTerminal();
    if (!term || !interactionEl) return;
    const maxTop = Math.max(0, interactionEl.scrollHeight - interactionEl.clientHeight);
    const linesUpFromBottom = Math.max(0, scrollbackLines() - viewportTopLine());
    const nextTop = Math.max(0, maxTop - linesUpFromBottom * cellHeightPx());
    if (Math.abs(interactionEl.scrollTop - nextTop) <= 1) {
      refreshFollow();
      return;
    }
    syncingScroll = true;
    interactionEl.scrollTop = nextTop;
    syncingScroll = false;
    refreshFollow();
  };

  const syncTermFromWrapper = () => {
    const term = getTerminal();
    if (!term || !interactionEl) return;
    const maxTop = Math.max(0, interactionEl.scrollHeight - interactionEl.clientHeight);
    if (interactionEl.scrollTop < maxTop - 1) {
      followLive = false;
    }
    const fromBottomPx = Math.max(0, maxTop - interactionEl.scrollTop);
    const linesUpFromBottom = Math.floor(fromBottomPx / cellHeightPx());
    const targetLine = Math.max(
      0,
      term.buffer.active.length - term.rows - linesUpFromBottom,
    );
    const clampedLine = Math.min(
      targetLine,
      Math.max(0, term.buffer.active.length - 1),
    );
    if (viewportTopLine() === clampedLine) {
      refreshFollow();
      return;
    }
    syncingScroll = true;
    wrapperDroveScroll = true;
    term.scrollToLine(clampedLine);
    syncingScroll = false;
    wrapperDroveScroll = false;
    refreshFollow();
  };

  const applyOutput = () => {
    syncSpacer();
    if (followLive) {
      syncingScroll = true;
      getTerminal()?.scrollToBottom();
      scrollInteractionToBottom();
      syncingScroll = false;
      refreshFollow();
    } else {
      onUnseenOutput(true);
    }
  };

  const onInteractionScroll = () => {
    if (syncingScroll) return;
    syncTermFromWrapper();
  };

  const onTermScroll = () => {
    if (syncingScroll || wrapperDroveScroll) return;
    syncWrapperFromTerm();
  };

  return {
    syncSpacer,
    scrollInteractionToBottom,
    refreshFollow,
    applyOutput,
    onInteractionScroll,
    onTermScroll,
    setFollowLive(v: boolean) {
      followLive = v;
    },
    setSyncingScroll(v: boolean) {
      syncingScroll = v;
    },
  };
}
