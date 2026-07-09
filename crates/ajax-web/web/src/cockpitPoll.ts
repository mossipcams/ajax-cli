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

export function createInFlightGuard(): {
  run<T>(fn: () => Promise<T>): Promise<T | undefined>;
} {
  let inFlight: Promise<unknown> | null = null;

  return {
    async run<T>(fn: () => Promise<T>): Promise<T | undefined> {
      if (inFlight) return undefined;
      const promise = fn().finally(() => {
        if (inFlight === promise) inFlight = null;
      });
      inFlight = promise;
      return promise;
    },
  };
}
