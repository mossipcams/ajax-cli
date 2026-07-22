export const KEY_REPEAT_INITIAL_DELAY_MS = 500;
export const KEY_REPEAT_INITIAL_INTERVAL_MS = 100;
export const KEY_REPEAT_MIN_INTERVAL_MS = 30;
export const KEY_REPEAT_STAGE_INTERVALS_MS = [100, 70, 50, 30] as const;

const EMITS_PER_STAGE = 4;

export function nextRepeatInterval(stage: number): number {
  const index = Math.min(
    Math.floor(stage / EMITS_PER_STAGE),
    KEY_REPEAT_STAGE_INTERVALS_MS.length - 1,
  );
  return KEY_REPEAT_STAGE_INTERVALS_MS[index];
}

type TimerHandle = ReturnType<typeof setTimeout>;

export function createHeldKeyRepeater({
  emit,
  isActive = () => true,
  setTimeout,
  clearTimeout,
}: {
  emit: () => void;
  isActive?: () => boolean;
  setTimeout: typeof globalThis.setTimeout;
  clearTimeout: typeof globalThis.clearTimeout;
}) {
  let running = false;
  let stage = 0;
  let initialTimer: TimerHandle | null = null;
  let repeatTimer: TimerHandle | null = null;

  const clearTimers = () => {
    if (initialTimer !== null) {
      clearTimeout(initialTimer);
      initialTimer = null;
    }
    if (repeatTimer !== null) {
      clearTimeout(repeatTimer);
      repeatTimer = null;
    }
  };

  const stop = () => {
    running = false;
    clearTimers();
  };

  const tryEmit = () => {
    if (!running || !isActive()) {
      stop();
      return;
    }
    emit();
    if (!running || !isActive()) {
      stop();
    }
  };

  const scheduleNext = () => {
    if (!running || !isActive()) {
      stop();
      return;
    }
    const interval = nextRepeatInterval(stage);
    stage += 1;
    repeatTimer = setTimeout(() => {
      repeatTimer = null;
      tryEmit();
      if (running) scheduleNext();
    }, interval);
  };

  const start = () => {
    stop();
    running = true;
    stage = 0;
    tryEmit();
    if (!running) return;
    initialTimer = setTimeout(() => {
      initialTimer = null;
      if (!running || !isActive()) {
        stop();
        return;
      }
      tryEmit();
      if (running) scheduleNext();
    }, KEY_REPEAT_INITIAL_DELAY_MS);
  };

  return { start, stop, isActive: () => running };
}
