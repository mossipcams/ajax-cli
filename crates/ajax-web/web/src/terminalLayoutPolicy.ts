export const EXPAND_REWRAP_MS = 280;

export type LayoutDecision = {
  allowLocalFit: boolean;
  allowPtyResize: boolean;
  cropToBottom: boolean;
  pinToBottomOnKeyboardOpen: boolean;
};

export type TerminalLayoutPolicyOptions = {
  now?: () => number;
  schedule?: (fn: () => void, delayMs: number) => ReturnType<typeof setTimeout>;
  clearSchedule?: (id: ReturnType<typeof setTimeout>) => void;
  raf?: (fn: () => void) => number;
  cancelRaf?: (id: number) => void;
};

export const createTerminalLayoutPolicy = (
  options: TerminalLayoutPolicyOptions = {},
) => {
  const now = options.now ?? (() => Date.now());
  const schedule =
    options.schedule ??
    ((fn: () => void, delayMs: number) => setTimeout(fn, delayMs));
  const clearSchedule = options.clearSchedule ?? clearTimeout;
  const raf = options.raf ?? ((fn: () => void) => requestAnimationFrame(fn));
  const cancelRaf = options.cancelRaf ?? cancelAnimationFrame;

  let keyboardOpen = false;
  let pinchActive = false;
  let expandActive = false;
  let pinToBottomPending = false;
  let expandTimer: ReturnType<typeof setTimeout> | undefined;
  let pinchRafIds: number[] = [];
  let expandRafIds: number[] = [];
  let disposed = false;

  const discreteIntentActive = () => pinchActive || expandActive;

  const makeDecision = (): LayoutDecision => {
    const intent = discreteIntentActive();
    const decision: LayoutDecision = {
      allowLocalFit: !keyboardOpen || intent,
      allowPtyResize: !keyboardOpen || intent,
      cropToBottom: keyboardOpen && !intent,
      pinToBottomOnKeyboardOpen: pinToBottomPending,
    };
    pinToBottomPending = false;
    return decision;
  };

  const clearPinchRafs = () => {
    for (const id of pinchRafIds) cancelRaf(id);
    pinchRafIds = [];
  };

  const clearExpandRafs = () => {
    for (const id of expandRafIds) cancelRaf(id);
    expandRafIds = [];
  };

  const clearExpandTimer = () => {
    if (expandTimer !== undefined) {
      clearSchedule(expandTimer);
      expandTimer = undefined;
    }
  };

  const schedulePinchClear = () => {
    clearPinchRafs();
    pinchRafIds.push(
      raf(() => {
        pinchRafIds.push(
          raf(() => {
            if (disposed) return;
            pinchActive = false;
          }),
        );
      }),
    );
  };

  const scheduleExpandClear = () => {
    clearExpandTimer();
    clearExpandRafs();
    const startedAt = now();
    expandTimer = schedule(() => {
      expandTimer = undefined;
      if (disposed) return;
      expandRafIds.push(
        raf(() => {
          expandRafIds.push(
            raf(() => {
              if (disposed) return;
              expandActive = false;
            }),
          );
        }),
      );
    }, Math.max(0, EXPAND_REWRAP_MS - (now() - startedAt)));
  };

  return {
    setKeyboardOpen(open: boolean): LayoutDecision {
      if (!keyboardOpen && open) pinToBottomPending = true;
      keyboardOpen = open;
      return makeDecision();
    },

    expandEnter(): LayoutDecision {
      expandActive = true;
      scheduleExpandClear();
      return makeDecision();
    },

    expandExit(): LayoutDecision {
      expandActive = false;
      clearExpandTimer();
      clearExpandRafs();
      return makeDecision();
    },

    pinchEnded(): LayoutDecision {
      pinchActive = true;
      // Defer clear scheduling so a same-turn schedulePostLayoutRefit registers
      // its refit rAFs first (matches legacy pinch-end exemption ordering).
      const scheduleClear = () => schedulePinchClear();
      if (typeof queueMicrotask === "function") {
        queueMicrotask(scheduleClear);
      } else {
        schedule(scheduleClear, 0);
      }
      return makeDecision();
    },

    decision(): LayoutDecision {
      return makeDecision();
    },

    dispose(): void {
      disposed = true;
      pinchActive = false;
      expandActive = false;
      pinToBottomPending = false;
      clearPinchRafs();
      clearExpandTimer();
      clearExpandRafs();
    },
  };
};
