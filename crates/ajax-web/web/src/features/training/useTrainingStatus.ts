import { useCallback, useEffect, useRef, useState } from "react";
import { fetchTrainingStatus, type TrainingStatus } from "./trainingApi";

export const TRAINING_STATUS_POLL_MS = 3000;

export interface UseTrainingStatus {
  status: TrainingStatus | null;
  error: string | null;
  refresh: () => Promise<void>;
}

/** Polls GET /api/training/status every 3000ms only while `enabled` is true. */
export function useTrainingStatus(enabled: boolean): UseTrainingStatus {
  const [status, setStatus] = useState<TrainingStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inFlightRef = useRef(false);

  const load = useCallback(async () => {
    if (inFlightRef.current) return;
    inFlightRef.current = true;
    try {
      setStatus(await fetchTrainingStatus());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      inFlightRef.current = false;
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
