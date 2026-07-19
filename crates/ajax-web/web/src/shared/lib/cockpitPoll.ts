import type { BrowserCockpitView } from "./types";

// API JSON is parsed with stable key order from serde; plain stringify is enough.
export function stableCockpitHash(view: BrowserCockpitView): string {
  return JSON.stringify(view);
}

export function createCockpitApplyGate(): {
  applyIfChanged(next: BrowserCockpitView): boolean;
  reset(): void;
} {
  let lastHash: string | null = null;

  return {
    applyIfChanged(next: BrowserCockpitView): boolean {
      const hash = stableCockpitHash(next);
      if (hash === lastHash) return false;
      lastHash = hash;
      return true;
    },
    reset() {
      lastHash = null;
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
