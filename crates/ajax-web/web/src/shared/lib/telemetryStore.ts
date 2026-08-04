export interface TelemetryQueuedEvent {
  event_id: string;
  event_name: string;
  properties: Record<string, string | number | boolean>;
  created_at: number;
  attempts: number;
  next_attempt_at: number;
}

export interface TelemetryStore {
  put(event: TelemetryQueuedEvent): Promise<void>;
  delete(eventId: string): Promise<void>;
  getReadyEvents(limit: number, now?: number): Promise<TelemetryQueuedEvent[]>;
  countPending(): Promise<number>;
  updateAfterFailure(
    eventId: string,
    attempts: number,
    nextAttemptAt: number,
  ): Promise<void>;
}

const DB_NAME = "ajax-telemetry";
const DB_VERSION = 1;
const STORE_NAME = "events";

/** In-memory store for unit tests; optional shared backing simulates IndexedDB persistence. */
export function createMemoryTelemetryStore(
  backing?: Map<string, TelemetryQueuedEvent>,
): TelemetryStore {
  const events = backing ?? new Map<string, TelemetryQueuedEvent>();

  return {
    async put(event) {
      events.set(event.event_id, { ...event });
    },
    async delete(eventId) {
      events.delete(eventId);
    },
    async getReadyEvents(limit, now = Date.now()) {
      return [...events.values()]
        .filter((event) => event.next_attempt_at <= now)
        .sort((a, b) => a.created_at - b.created_at)
        .slice(0, limit);
    },
    async countPending() {
      return events.size;
    },
    async updateAfterFailure(eventId, attempts, nextAttemptAt) {
      const existing = events.get(eventId);
      if (!existing) {
        return;
      }
      events.set(eventId, { ...existing, attempts, next_attempt_at: nextAttemptAt });
    },
  };
}

function createIdbStore(db: IDBDatabase): TelemetryStore {
  const txStore = () => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    return tx.objectStore(STORE_NAME);
  };

  return {
    async put(event) {
      await new Promise<void>((resolve, reject) => {
        const request = txStore().put(event);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
      });
    },
    async delete(eventId) {
      await new Promise<void>((resolve, reject) => {
        const request = txStore().delete(eventId);
        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
      });
    },
    async getReadyEvents(limit, now = Date.now()) {
      return new Promise((resolve, reject) => {
        const results: TelemetryQueuedEvent[] = [];
        const request = txStore().openCursor();
        request.onsuccess = () => {
          const cursor = request.result;
          if (!cursor) {
            results.sort((a, b) => a.created_at - b.created_at);
            resolve(results.slice(0, limit));
            return;
          }
          const event = cursor.value as TelemetryQueuedEvent;
          if (event.next_attempt_at <= now) {
            results.push(event);
          }
          cursor.continue();
        };
        request.onerror = () => reject(request.error);
      });
    },
    async countPending() {
      return new Promise((resolve, reject) => {
        const request = txStore().count();
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
    },
    async updateAfterFailure(eventId, attempts, nextAttemptAt) {
      await new Promise<void>((resolve, reject) => {
        const store = txStore();
        const getRequest = store.get(eventId);
        getRequest.onsuccess = () => {
          const existing = getRequest.result as TelemetryQueuedEvent | undefined;
          if (!existing) {
            resolve();
            return;
          }
          const putRequest = store.put({
            ...existing,
            attempts,
            next_attempt_at: nextAttemptAt,
          });
          putRequest.onsuccess = () => resolve();
          putRequest.onerror = () => reject(putRequest.error);
        };
        getRequest.onerror = () => reject(getRequest.error);
      });
    },
  };
}

export function openIndexedDbTelemetryStore(): Promise<TelemetryStore> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onerror = () => reject(request.error);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: "event_id" });
      }
    };
    request.onsuccess = () => resolve(createIdbStore(request.result));
  });
}
