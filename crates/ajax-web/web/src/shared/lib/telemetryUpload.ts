import type { TelemetryStore } from "./telemetryStore";

export const DEFAULT_BATCH_SIZE = 20;
export const BASE_BACKOFF_MS = 1_000;
export const MAX_BACKOFF_MS = 5 * 60 * 1_000;

export type TelemetryCaptureFn = (
  event: string,
  properties: Record<string, string | number | boolean>,
) => void | Promise<void>;

export function computeNextAttemptAt(attempts: number, now = Date.now()): number {
  const delay = Math.min(MAX_BACKOFF_MS, BASE_BACKOFF_MS * 2 ** attempts);
  return now + delay;
}

/**
 * Upload ready queued events in batches. Deletes each record only after its
 * capture succeeds. Returns true when a full batch was processed (more may remain).
 */
export async function flushTelemetryQueue(
  store: TelemetryStore,
  captureFn: TelemetryCaptureFn,
  opts?: { batchSize?: number; now?: number },
): Promise<boolean> {
  const batchSize = opts?.batchSize ?? DEFAULT_BATCH_SIZE;
  const now = opts?.now ?? Date.now();
  const events = await store.getReadyEvents(batchSize, now);
  if (events.length === 0) {
    return false;
  }

  for (const event of events) {
    try {
      await captureFn(event.event_name, event.properties);
      await store.delete(event.event_id);
    } catch {
      const attempts = event.attempts + 1;
      await store.updateAfterFailure(
        event.event_id,
        attempts,
        computeNextAttemptAt(attempts, now),
      );
    }
  }

  return events.length === batchSize;
}
