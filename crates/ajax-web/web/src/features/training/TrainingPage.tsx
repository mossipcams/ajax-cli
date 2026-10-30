import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Button } from "@/shared/ui/button";
import Skeleton from "@/shared/ui/Skeleton";
import { useTrainingStatus } from "./useTrainingStatus";
import * as trainingApi from "./trainingApi";
import TrainingPipeline from "./TrainingPipeline";
import TrainingDatasets from "./TrainingDatasets";
import TrainingEvals from "./TrainingEvals";
import {
  busyReason, busyText, confirmDescription, evalEtaSeconds, finishClock, finishLabel, formatElapsed, formatEta,
  jobLabel, lossSeries, messageOf, rowsEtaSeconds, stepsPerMinute,
  useTrainingCatalog, type PageAction,
} from "./trainingPageModel";

interface Props {
  onBack: () => void;
}

const LOG_TAIL_LINES = 40;

function RunKpis({ run }: { run: trainingApi.TrainingRun }) {
  const progress = run.progress;
  const losses = lossSeries(run.log_tail);
  const first = losses[0] ?? null;
  const last = losses.length > 0 ? losses[losses.length - 1] : null;
  const lossValue = progress?.loss ?? last;
  const rate = progress ? stepsPerMinute(progress.step, run.started) : null;
  const finish = progress?.eta_s != null ? finishClock(progress.eta_s) : null;
  const delta = first !== null && last !== null && losses.length >= 2 ? last - first : null;
  const deltaClass = delta === null || delta === 0 ? "" : delta < 0 ? "is-good" : "is-bad";
  const deltaArrow = delta === null || delta === 0 ? "→" : delta < 0 ? "↓" : "↑";
  const percent = progress?.total ? Math.round((progress.step / progress.total) * 100) : null;

  return (
    <>
      <dl className="training-kpis" aria-label="Run stats">
        <div className="training-kpi">
          <dt>Progress</dt>
          <dd>{percent !== null ? `${percent}%` : "—"}</dd>
          {progress ? (
            <dd className="training-kpi-note">
              {progress.total
                ? `${progress.step.toLocaleString()} / ${progress.total.toLocaleString()} steps`
                : `${progress.step.toLocaleString()} steps`}
            </dd>
          ) : null}
        </div>
        <div className="training-kpi">
          <dt>Finish</dt>
          <dd>{finish ? `~${finish}` : "—"}</dd>
          {progress?.eta_s != null ? <dd className="training-kpi-note">{`in ${formatEta(progress.eta_s)}`}</dd> : null}
        </div>
        <div className="training-kpi">
          <dt>Speed</dt>
          <dd>{rate !== null ? rate.toFixed(1) : "—"}</dd>
          {rate !== null ? <dd className="training-kpi-note">steps/min</dd> : null}
        </div>
        <div className="training-kpi">
          <dt>Loss</dt>
          <dd>{lossValue !== null ? lossValue.toFixed(4) : "—"}</dd>
          {delta !== null ? (
            <dd className={`training-kpi-note ${deltaClass}`}>
              {`${deltaArrow} ${Math.abs(delta).toFixed(4)} since first logged step`}
            </dd>
          ) : null}
        </div>
      </dl>
      {losses.length >= 3 ? <LossTrend losses={losses} /> : null}
    </>
  );
}

function LossTrend({ losses }: { losses: number[] }) {
  const min = Math.min(...losses);
  const max = Math.max(...losses);
  const range = max - min || 1;
  const points = losses.map((loss, index) => {
    const x = (index / (losses.length - 1)) * 100;
    const y = 30 - ((loss - min) / range) * 28;
    return { x, y };
  });
  const lastPoint = points[points.length - 1];
  const first = losses[0].toFixed(4);
  const latest = losses[losses.length - 1].toFixed(4);
  return (
    <figure className="training-loss" aria-label="Loss trend">
      <div className="training-loss-plot">
        <svg viewBox="0 0 100 32" preserveAspectRatio="none" role="img">
          <title>{`Loss over the last ${losses.length} logged steps: ${first} → ${latest}`}</title>
          <polyline
            fill="none"
            points={points.map((point) => `${point.x.toFixed(2)},${point.y.toFixed(2)}`).join(" ")}
            vector-effect="non-scaling-stroke"
          />
        </svg>
        <span
          className="training-loss-dot"
          aria-hidden="true"
          style={{ left: `${lastPoint.x}%`, top: `${(lastPoint.y / 32) * 100}%` }}
        />
      </div>
      <figcaption>
        <span>First logged</span>
        <span>Now</span>
      </figcaption>
    </figure>
  );
}

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
  const jobActive = runRunning || genRunning || evalRunning;
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

  const genBaselineRef = useRef<{ rows: number; atMs: number } | null>(null);
  useEffect(() => {
    if (!genRunning) {
      genBaselineRef.current = null;
      return;
    }
    if (genBaselineRef.current === null) genBaselineRef.current = { rows: generation?.rows ?? 0, atMs: Date.now() };
  }, [genRunning, generation?.rows]);

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

  let etaSeconds: number | null = null;
  if (run?.running && run.kind !== "generate" && progress?.eta_s != null) {
    etaSeconds = progress.eta_s;
  } else if (genRunning) {
    const baseline = genBaselineRef.current;
    if (generation !== null && baseline !== null) {
      etaSeconds = rowsEtaSeconds(baseline, { rows: generation.rows ?? 0, atMs: Date.now() }, generation.target);
    }
  } else if (evalRunning) {
    etaSeconds = evalEtaSeconds(status?.eval_progress ?? []);
  }

  const logText = logTail.join("\n");
  const logRef = useRef<HTMLPreElement | null>(null);
  const stickToBottomRef = useRef(true);

  function handleLogScroll(): void {
    const el = logRef.current;
    if (!el) return;
    stickToBottomRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 24;
  }

  function handleLogDisclosureToggle(event: { currentTarget: HTMLDetailsElement }): void {
    const logEl = logRef.current;
    if (event.currentTarget.open && logEl) logEl.scrollTop = logEl.scrollHeight;
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

          {jobActive ? (
            <div className="training-page-eta" data-testid="training-page-eta">
              {etaSeconds !== null ? (
                <>
                  <p className="training-page-eta-main">Done around {finishLabel(etaSeconds)}</p>
                  <p className="training-page-eta-sub">{formatEta(etaSeconds)} left</p>
                </>
              ) : (
                <p className="training-page-eta-main">Estimating finish time…</p>
              )}
            </div>
          ) : null}

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
                {runElapsed ? ` · ${runElapsed} elapsed` : ""}
              </p>
            </>
          ) : null}

          {run?.running && run.kind !== "generate" ? (
            <details className="training-disclosure">
              <summary>Run details</summary>
              <RunKpis run={run} />
            </details>
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

      {status && busyLine ? (
        <p className="training-muted" data-testid="training-page-busy">
          Jobs unavailable: {busyLine}
        </p>
      ) : null}

      {status ? (
        <details
          className="training-disclosure"
          data-testid="training-disclosure-pipeline"
        >
          <summary>Pipeline</summary>
          <TrainingPipeline
            status={status}
            championRunId={championRunId}
            jobsDisabled={jobsDisabled}
            evals={catalog.evals}
            datasets={catalog.datasets}
            onAction={setPending}
          />
        </details>
      ) : null}

      <details className="training-disclosure" data-testid="training-disclosure-datasets">
        <summary>Datasets</summary>
        <TrainingDatasets
          datasets={catalog.datasets}
          promotedRunId={catalog.catalogPromotedRunId}
          selectDisabled={selectDisabled}
          onAction={setPending}
        />
      </details>

      <details className="training-disclosure" data-testid="training-disclosure-evals">
        <summary>Evals</summary>
        <TrainingEvals evals={catalog.evals} progress={status?.eval_progress ?? []} />
      </details>

      {run ? (
        <details
          className="training-disclosure"
          data-testid="training-disclosure-log"
          onToggle={handleLogDisclosureToggle}
        >
          <summary>Log</summary>
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
        </details>
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
