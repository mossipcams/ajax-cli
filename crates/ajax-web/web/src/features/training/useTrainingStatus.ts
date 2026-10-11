import { useCallback, useEffect, useRef, useState } from "react";
import { fetchTrainingStatus, type TrainingStatus } from "./trainingApi";

const TRAINING_STATUS_POLL_MS = 3000;

export interface UseTrainingStatus {
  status: TrainingStatus | null;
  error: string | null;
  refresh: () => Promise<void>;
}

export function useTrainingStatus(enabled: boolean): UseTrainingStatus {
  const [status, setStatus] = useState<TrainingStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inFlightRef = useRef(false);
  // A manual refresh arriving while a fetch is in flight queues exactly one
  // follow-up so post-action state always lands instead of waiting for the
  // next 3s poll (at most one queued; a follow-up after a failure is left to
  // the next scheduled poll, so an unreachable host keeps its cadence).
  const refetchQueuedRef = useRef(false);

  const load = useCallback(async () => {
    if (inFlightRef.current) {
      refetchQueuedRef.current = true;
      return;
    }
    inFlightRef.current = true;
    let succeeded = false;
    try {
      setStatus(await fetchTrainingStatus());
      setError(null);
      succeeded = true;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      inFlightRef.current = false;
      if (refetchQueuedRef.current && succeeded) {
        refetchQueuedRef.current = false;
        void load();
      } else {
        refetchQueuedRef.current = false;
      }
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    void load();
    const timer = window.setInterval(() => void load(), TRAINING_STATUS_POLL_MS);
    return () => window.clearInterval(timer);
  }, [enabled, load]);

  return { status, error, refresh: load };
}
