import { describe, expect, it } from "vitest";
import {
  createMemoryTelemetryStore,
  type TelemetryQueuedEvent,
} from "./telemetryStore";

function sampleEvent(overrides: Partial<TelemetryQueuedEvent> = {}): TelemetryQueuedEvent {
  return {
    event_id: "evt-1",
    event_name: "ajax_swipe",
    properties: { direction: "left" },
    created_at: 100,
    attempts: 0,
    next_attempt_at: 0,
    ...overrides,
  };
}

describe("createMemoryTelemetryStore", () => {
  it("persists events across a simulated store reopen", async () => {
    const backing = new Map<string, TelemetryQueuedEvent>();
    const store1 = createMemoryTelemetryStore(backing);
    await store1.put(sampleEvent());

    const store2 = createMemoryTelemetryStore(backing);
    expect(await store2.countPending()).toBe(1);
    const ready = await store2.getReadyEvents(10);
    expect(ready).toHaveLength(1);
    expect(ready[0]?.event_name).toBe("ajax_swipe");
  });

  it("returns only ready events respecting next_attempt_at", async () => {
    const store = createMemoryTelemetryStore();
    await store.put(sampleEvent({ event_id: "ready", next_attempt_at: 0 }));
    await store.put(
      sampleEvent({
        event_id: "later",
        next_attempt_at: 1_000,
        created_at: 200,
      }),
    );

    const ready = await store.getReadyEvents(10, 500);
    expect(ready.map((event) => event.event_id)).toEqual(["ready"]);
  });

  it("orders ready events by created_at", async () => {
    const store = createMemoryTelemetryStore();
    await store.put(sampleEvent({ event_id: "b", created_at: 200 }));
    await store.put(sampleEvent({ event_id: "a", created_at: 100 }));

    const ready = await store.getReadyEvents(10);
    expect(ready.map((event) => event.event_id)).toEqual(["a", "b"]);
  });

  it("updates failure metadata without deleting the record", async () => {
    const store = createMemoryTelemetryStore();
    await store.put(sampleEvent());

    await store.updateAfterFailure("evt-1", 2, 9_999);

    expect(await store.countPending()).toBe(1);
    expect(await store.getReadyEvents(10, 0)).toHaveLength(0);
    const later = await store.getReadyEvents(10, 9_999);
    expect(later[0]?.attempts).toBe(2);
    expect(later[0]?.next_attempt_at).toBe(9_999);
  });

  it("deletes events after successful delivery", async () => {
    const store = createMemoryTelemetryStore();
    await store.put(sampleEvent());
    await store.delete("evt-1");
    expect(await store.countPending()).toBe(0);
  });
});
