import { describe, expect, it, vi } from "vitest";
import {
  BASE_BACKOFF_MS,
  computeNextAttemptAt,
  flushTelemetryQueue,
  MAX_BACKOFF_MS,
} from "./telemetryUpload";
import { createMemoryTelemetryStore, type TelemetryQueuedEvent } from "./telemetryStore";

function queued(
  id: string,
  overrides: Partial<TelemetryQueuedEvent> = {},
): TelemetryQueuedEvent {
  return {
    event_id: id,
    event_name: "ajax_swipe",
    properties: { direction: "left" },
    created_at: Number(id.replace(/\D/g, "")) || 0,
    attempts: 0,
    next_attempt_at: 0,
    ...overrides,
  };
}

describe("computeNextAttemptAt", () => {
  it("applies exponential backoff capped at MAX_BACKOFF_MS", () => {
    const now = 1_000;
    expect(computeNextAttemptAt(1, now)).toBe(now + BASE_BACKOFF_MS * 2);
    expect(computeNextAttemptAt(20, now)).toBe(now + MAX_BACKOFF_MS);
  });
});

describe("flushTelemetryQueue", () => {
  it("captures ready events in batches and deletes only after success", async () => {
    const store = createMemoryTelemetryStore();
    for (let i = 1; i <= 3; i += 1) {
      await store.put(queued(`evt-${i}`, { created_at: i }));
    }
    const capture = vi.fn();

    const more = await flushTelemetryQueue(store, capture, { batchSize: 2 });
    expect(capture).toHaveBeenCalledTimes(2);
    expect(await store.countPending()).toBe(1);
    expect(more).toBe(true);

    const moreAgain = await flushTelemetryQueue(store, capture, { batchSize: 2 });
    expect(capture).toHaveBeenCalledTimes(3);
    expect(await store.countPending()).toBe(0);
    expect(moreAgain).toBe(false);
  });

  it("keeps failed events and schedules retry with backoff", async () => {
    const store = createMemoryTelemetryStore();
    await store.put(queued("evt-1"));
    const now = 5_000;
    const capture = vi.fn(() => {
      throw new Error("network down");
    });

    await flushTelemetryQueue(store, capture, { now });

    expect(await store.countPending()).toBe(1);
    const retryAt = computeNextAttemptAt(1, now);
    const pending = await store.getReadyEvents(10, retryAt);
    expect(pending[0]?.attempts).toBe(1);
    expect(pending[0]?.next_attempt_at).toBe(retryAt);

    const notReady = await store.getReadyEvents(10, now + 1);
    expect(notReady).toHaveLength(0);
  });

  it("retries after backoff elapses", async () => {
    const store = createMemoryTelemetryStore();
    const now = 10_000;
    await store.put(
      queued("evt-1", {
        attempts: 1,
        next_attempt_at: computeNextAttemptAt(1, now - BASE_BACKOFF_MS * 2),
      }),
    );
    const capture = vi.fn();

    await flushTelemetryQueue(store, capture, { now });

    expect(capture).toHaveBeenCalledTimes(1);
    expect(await store.countPending()).toBe(0);
  });
});
