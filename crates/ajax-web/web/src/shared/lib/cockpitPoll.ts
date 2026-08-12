import type { BrowserCockpitView } from "./types";

export type GestureBusyGate = {
  begin(): void;
  end(): void;
  isBusy(): boolean;
  onIdle(listener: () => void): () => void;
};

/** Refcount gate: while busy, background poll projections defer (INP). */
export function createGestureBusyGate(): GestureBusyGate {
  let count = 0;
  const idleListeners = new Set<() => void>();

  return {
    begin() {
      count += 1;
    },
    end() {
      if (count <= 0) return;
      count -= 1;
      if (count === 0) {
        for (const listener of idleListeners) listener();
      }
    },
    isBusy() {
      return count > 0;
    },
    onIdle(listener) {
      idleListeners.add(listener);
      return () => idleListeners.delete(listener);
    },
  };
}

export const gestureBusyGate = createGestureBusyGate();

// API JSON is parsed with stable key order from serde; plain stringify is enough.
export function stableCockpitHash(view: BrowserCockpitView): string {
  return JSON.stringify(view);
}

export function createCockpitApplyGate(): {
  applyIfChanged(next: BrowserCockpitView): boolean;
  noteMutation(): void;
  pollGeneration(): number;
  applyPollIfChanged(next: BrowserCockpitView, startedAt: number): boolean;
  reset(): void;
} {
  let lastHash: string | null = null;
  let generation = 0;

  return {
    applyIfChanged(next: BrowserCockpitView): boolean {
      const hash = stableCockpitHash(next);
      if (hash === lastHash) return false;
      lastHash = hash;
      return true;
    },
    noteMutation() {
      generation += 1;
    },
    pollGeneration() {
      return generation;
    },
    applyPollIfChanged(next: BrowserCockpitView, startedAt: number): boolean {
      if (startedAt !== generation) return false;
      const hash = stableCockpitHash(next);
      if (hash === lastHash) return false;
      lastHash = hash;
      return true;
    },
    reset() {
      lastHash = null;
      generation = 0;
    },
  };
}

export type InFlightRunOptions = {
  /** When true, overlapping calls schedule one trailing re-run after the flight. */
  trailing?: boolean;
};

export function createInFlightGuard(): {
  run<T>(fn: () => Promise<T>, options?: InFlightRunOptions): Promise<T | undefined>;
} {
  let inFlight: Promise<unknown> | null = null;
  let dirty = false;

  async function run<T>(
    fn: () => Promise<T>,
    options?: InFlightRunOptions,
  ): Promise<T | undefined> {
    if (inFlight) {
      if (options?.trailing) dirty = true;
      return undefined;
    }
    const promise = (async () => {
      let result!: T;
      do {
        dirty = false;
        result = await fn();
      } while (dirty);
      return result;
    })();
    inFlight = promise;
    try {
      return await promise;
    } finally {
      if (inFlight === promise) {
        const again = dirty;
        dirty = false;
        inFlight = null;
        // Trailing overlap arrived after the loop exited but before clear.
        if (again) void run(fn, { trailing: true });
      }
    }
  }

  return { run };
}
