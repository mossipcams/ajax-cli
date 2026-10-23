import { useCallback, useEffect, useRef, useState } from "react";
import * as trainingApi from "./trainingApi";

export type PageAction =
  | { kind: "job"; job: trainingApi.TrainingJobKind }
  | { kind: "stop" }
  | { kind: "select"; runId: string };

const CATALOG_POLL_MS = 15000;

export function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function jobLabel(kind: string): string {
  return trainingApi.TRAINING_JOB_LABELS[kind as trainingApi.TrainingJobKind] ?? kind;
}

export function confirmDescription(action: PageAction): string {
  switch (action.kind) {
    case "job":
      return `Start the ${jobLabel(action.job)} job?`;
    case "stop":
      return "Stop the current training run?";
    case "select":
      return `Select ${action.runId} as the current dataset?`;
  }
}

export function busyText(action: PageAction): string {
  switch (action.kind) {
    case "job":
      return `Starting ${jobLabel(action.job)}`;
    case "stop":
      return "Stopping run";
    case "select":
      return `Selecting ${action.runId}`;
  }
}

export function formatEta(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  return minutes >= 1 ? `~${minutes} min` : `~${Math.max(1, Math.round(seconds))}s`;
}

const EVAL_GROUP_RUN_ID_RE = /^\d{8}T\d{6}Z-[0-9a-f]{8}/;

export function evalGroupLabelParts(group: string): { primary: string; qualifier: string | null } {
  if (EVAL_GROUP_RUN_ID_RE.test(group)) {
    const primary = group.slice(0, 25);
    const rest = group.slice(25);
    const qualifier = rest.length > 0 ? rest.replace(/^[-_]/, "") || rest : null;
    return { primary, qualifier };
  }
  return { primary: group, qualifier: null };
}

export function pathBasename(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const slash = normalized.lastIndexOf("/");
  return slash >= 0 ? normalized.slice(slash + 1) : path;
}

export function shortRunId(runId: string): string {
  const dash = runId.indexOf("-");
  return dash >= 0 ? runId.slice(dash + 1) : runId;
}

export function useTrainingCatalog() {
  const [datasets, setDatasets] = useState<trainingApi.TrainingDataset[] | null>(null);
  const [catalogPromotedRunId, setCatalogPromotedRunId] = useState<string | null>(null);
  const [evals, setEvals] = useState<trainingApi.TrainingEvals | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refreshGenerationRef = useRef(0);

  const refresh = useCallback(async () => {
    const generation = ++refreshGenerationRef.current;
    const [datasetsResult, evalsResult] = await Promise.allSettled([
      trainingApi.fetchTrainingDatasets(),
      trainingApi.fetchTrainingEvals(),
    ]);
    if (generation !== refreshGenerationRef.current) return;
    if (datasetsResult.status === "fulfilled") {
      setDatasets(datasetsResult.value.datasets);
      setCatalogPromotedRunId(datasetsResult.value.promoted_run_id);
    }
    if (evalsResult.status === "fulfilled") setEvals(evalsResult.value);
    const failure = [datasetsResult, evalsResult].find((result) => result.status === "rejected");
    setError(failure?.status === "rejected" ? messageOf(failure.reason) : null);
  }, []);

  // Polls only while the page is mounted; the interval is cleared on unmount.
  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), CATALOG_POLL_MS);
    return () => window.clearInterval(timer);
  }, [refresh]);

  return { datasets, catalogPromotedRunId, evals, error, refresh };
}

