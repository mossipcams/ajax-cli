import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button } from "@/shared/ui/button";
import Skeleton from "@/shared/ui/Skeleton";
import { useTrainingStatus } from "./useTrainingStatus";
import * as trainingApi from "./trainingApi";
import TrainingPipeline from "./TrainingPipeline";
import TrainingDatasets from "./TrainingDatasets";
import TrainingEvals from "./TrainingEvals";
import {
  busyReason, busyText, confirmDescription, formatElapsed, formatEta, jobLabel,
  messageOf,
  useTrainingCatalog, type PageAction,
} from "./trainingPageModel";

interface Props {
  onBack: () => void;
}

const LOG_TAIL_LINES = 40;

export default function TrainingPage({ onBack }: Props) {
  const { status, error: statusError, refresh: refreshStatus } = useTrainingStatus(true);
  const catalog = useTrainingCatalog();
  const { refresh: refreshCatalog } = catalog;
  const [pending, setPending] = useState<PageAction | null>(null);
  const [active, setActive] = useState<PageAction | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const run = status?.run ?? null;
  const generation = status?.generation ?? null;
  const runRunning = Boolean(run?.running);
  const genRunning = Boolean(generation?.running);
  const evalRunning = Boolean(status?.eval_progress.length);
  const busy = active !== null;
  const champion = catalog.evals?.champion ?? null;
  const championRunId = champion?.run_id ?? null;

  const wasRunningRef = useRef(false);
  const previousEvalsRef = useRef<trainingApi.TrainingEvalProgress[]>([]);
  useEffect(() => {
    const currentEvals = status?.eval_progress ?? [];
    const evalFinished = previousEvalsRef.current.some((previous) =>
      !currentEvals.some((entry) => entry.group === previous.group &&
        entry.checkpoint_label === previous.checkpoint_label),
    );
    if ((wasRunningRef.current && !runRunning) || evalFinished) void refreshCatalog();
    wasRunningRef.current = runRunning;
    previousEvalsRef.current = currentEvals;
  }, [runRunning, status?.eval_progress, refreshCatalog]);

  const actionInFlightRef = useRef(false);

  async function execute(action: PageAction): Promise<void> {
    if (actionInFlightRef.current) return;
    actionInFlightRef.current = true;
    setActive(action);
    setActionError(null);
    try {
      if (action.kind === "job") await trainingApi.startTrainingJob(action.job);
      else if (action.kind === "stop") await trainingApi.stopTraining();
      else await trainingApi.selectTrainingDataset(action.runId);
      setPending(null);
      void refreshStatus();
      void catalog.refresh();
    } catch (err) {
      setActionError(messageOf(err));
    } finally {
      actionInFlightRef.current = false;
      setActive(null);
    }
  }

  const confirmInFlight = busy && active !== null && pending === active;
  const jobsDisabled = runRunning || genRunning || evalRunning || busy;
  const trainStateBusy = Boolean(status?.state?.startsWith("train:"));
  const selectDisabled = jobsDisabled || trainStateBusy;
  const pendingDisabled = pending?.kind === "stop"
    ? busy || !runRunning || run?.external === true
    : pending?.kind === "select" ? selectDisabled : jobsDisabled;

  const deviceLabel = status?.device === "gpu" ? "GPU" : status?.device === "cpu" ? "CPU" : "Idle";
  const deviceClass = status?.device ?? "idle";
  const progress = run?.progress ?? null;
  const logTail = run ? run.log_tail.slice(-LOG_TAIL_LINES) : [];

  const runElapsed = run && run.running ? formatElapsed(run.started) : null;
  const busyLine = busyReason(status);

  const logText = logTail.join("\n");
  const logRef = useRef<HTMLPreElement | null>(null);
  const stickToBottomRef = useRef(true);

  function handleLogScroll(): void {
    const el = logRef.current;
    if (!el) return;
    stickToBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 24;
  }

  useLayoutEffect(() => {
    const el = logRef.current;
    if (el && stickToBottomRef.current) el.scrollTop = el.scrollHeight;
  }, [logText]);

  return (
    <section className="training-page" aria-labelledby="training-page-heading" data-testid="training-page">
      <div className="training-page-header">
        <Button type="button" variant="secondary" className="training-back" onClick={onBack}>
          Back
        </Button>
        <h2 id="training-page-heading">Training</h2>
      </div>

      {statusError && !status ? (
        <p role="alert" className="training-banner" data-testid="training-page-status-error">
          {statusError}
        </p>
      ) : null}

      {!status && !statusError ? <Skeleton rows={3} testid="training-page-loading" /> : null}

      {status ? (
        <section className="training-section training-page-status" aria-label="Training status" data-testid="training-page-status">
          <div className="training-page-status-row">
            <span className={`training-device is-${deviceClass}`} data-testid="training-device">
              {deviceLabel}
            </span>
            <span className="training-muted" title={status.state} data-testid="training-page-host">
              {status.runtime_up === true ? "Runtime up" : "Runtime down"}
              {status.active_profile ? ` · ${status.active_profile}` : ""}
            </span>
          </div>

          {run ? (
            <p className="training-page-run" data-testid="training-page-run">
              <strong>{jobLabel(run.kind)}</strong> {run.running ? "running" : "last run finished"}
              {run.external ? <span className="training-muted"> · started outside Ajax</span> : null}
            </p>
          ) : genRunning ? (
            <p className="training-muted" data-testid="training-page-gen-external">
              Data generation running · started outside Ajax
            </p>
          ) : (
            <p className="training-muted">No run in progress.</p>
          )}

          {genRunning ? (
            <div className="training-page-generation" data-testid="training-page-generation">
              <p className="training-muted">
                {generation && generation.target !== null
                  ? `Rows ${generation.rows ?? 0} of ${generation.target}`
                  : `${generation?.rows ?? 0} rows`}
                {generation?.phase ? ` · ${generation.phase}` : ""}
              </p>
              {generation && generation.target !== null ? (
                <progress
                  className="training-page-progress"
                  aria-label="Data generation progress"
                  value={generation.rows ?? 0}
                  max={generation.target}
                />
              ) : null}
            </div>
          ) : null}

          {run?.running && run.kind !== "generate" ? (
            <>
              <progress
                className="training-page-progress"
                aria-label="Training progress"
                data-testid="training-page-progress"
                {...(progress?.total ? { value: progress.step, max: progress.total } : {})}
              />
              <p className="training-muted">
                {progress?.total ? `Step ${progress.step} of ${progress.total}` : progress ? `Step ${progress.step}` : "Waiting for progress"}
                {progress?.loss != null ? ` · loss ${progress.loss.toFixed(4)}` : ""}
                {progress?.eta_s != null ? ` · ETA ${formatEta(progress.eta_s)}` : ""}
                {runElapsed ? ` · ${runElapsed} elapsed` : ""}
              </p>
            </>
          ) : null}

          {run?.running && !run.external ? (
            <Button
              type="button"
              variant="destructive"
              disabled={busy}
              onClick={() => setPending({ kind: "stop" })}
            >
              Stop
            </Button>
          ) : null}
        </section>
      ) : null}

      {status ? (
        <>
          {busyLine ? (
            <p className="training-muted" data-testid="training-page-busy">
              Jobs unavailable: {busyLine}
            </p>
          ) : null}
          <TrainingPipeline
            status={status}
            championRunId={championRunId}
            jobsDisabled={jobsDisabled}
            evals={catalog.evals}
            datasets={catalog.datasets}
            onAction={setPending}
          />
        </>
      ) : null}
      <TrainingDatasets
        datasets={catalog.datasets}
        promotedRunId={catalog.catalogPromotedRunId}
        selectDisabled={selectDisabled}
        onAction={setPending}
      />
      <TrainingEvals evals={catalog.evals} progress={status?.eval_progress ?? []} />

      {run ? (
        <section
          className="training-section"
          aria-labelledby="training-log-heading"
          data-testid="training-page-log-section"
        >
          <h3 id="training-log-heading">Log</h3>
          <pre
            className="training-page-log"
            data-testid="training-page-log"
            ref={logRef}
            onScroll={handleLogScroll}
          >
            {logText.length > 0 ? logText : "No log lines yet."}
          </pre>
        </section>
      ) : null}

      {catalog.error ? (
        <p role="alert" className="training-banner" data-testid="training-page-catalog-error">
          {catalog.error}
        </p>
      ) : null}

      {pending || actionError ? (
        <div className="training-page-dock" data-testid="training-page-dock">
          {pending ? (
            <div className="training-confirm" data-testid="training-page-confirm">
              <p className="training-confirm-text">{confirmDescription(pending)}</p>
              <div className="training-confirm-buttons">
                <Button
                  type="button"
                  disabled={pendingDisabled}
                  aria-busy={confirmInFlight || undefined}
                  onClick={() => void execute(pending)}
                >
                  {confirmInFlight ? (
                    <>
                      <span className="training-busy-dot" aria-hidden />
                      {busyText(active)}…
                    </>
                  ) : (
                    "Confirm"
                  )}
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => setPending(null)}
                >
                  Cancel
                </Button>
              </div>
            </div>
          ) : null}

          {actionError ? (
            <p role="alert" className="training-banner" data-testid="training-page-action-error">
              {actionError}
            </p>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
