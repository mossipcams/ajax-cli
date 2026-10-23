import { Button } from "@/shared/ui/button";

import * as trainingApi from "./trainingApi";
import { formatElapsed, formatEta } from "./trainingPageModel";

export interface TrainingStatusPanelProps {
  heroMode: "run" | "generation" | "live" | "idle";
  heroTitle: string;
  state: string;
  run: trainingApi.TrainingRun | null;
  generation: trainingApi.TrainingGeneration | null;
  runtimeUp: boolean;
  activeProfile: string | null;
  busy: boolean;
  runRunning: boolean;
  genRunning: boolean;
  onStop: () => void;
  onServeStop: () => void;
  onTrain: () => void;
}

function progressWidth(fraction: number): string {
  const percent = Math.min(100, Math.max(0, fraction * 100));
  return `${Number(percent.toFixed(1))}%`;
}

function formatStarted(started: string | null): string {
  if (!started) return "";
  const startedMs = Date.parse(started);
  if (!Number.isFinite(startedMs)) return "";
  const text = new Date(startedMs).toLocaleString();
  const elapsed = formatElapsed(started);
  return elapsed ? `${text} · ${elapsed}` : text;
}

export default function TrainingStatusPanel({
  heroMode,
  heroTitle,
  state,
  run,
  generation,
  runtimeUp,
  activeProfile,
  busy,
  runRunning,
  genRunning,
  onStop,
  onServeStop,
  onTrain,
}: TrainingStatusPanelProps) {
  return (
    // Status hero: what is happening right now, with raw state as tooltip only.
    <section
      className={`training-hero is-${heroMode}`}
      data-testid="training-hero"
      title={state}
      aria-label={`Status: ${state}`}
    >
      <span className="training-sr-only" data-testid="training-raw-state">
        {state}
      </span>

      {heroMode === "run" && run ? (
        <>
          <h3 className="training-hero-title" data-testid="training-state">
            {heroTitle}
          </h3>
          <div className="training-hero-body">
            <div className="training-hero-stats">
              {run.progress ? (
                <>
                  <div className="training-stat">
                    <span className="training-stat-label">Step</span>
                    <span className="training-stat-value" data-testid="training-step">
                      step {run.progress.step}
                      {run.progress.total !== null ? ` / ${run.progress.total}` : ""}
                    </span>
                  </div>
                  {run.progress.loss !== null ? (
                    <div className="training-stat">
                      <span className="training-stat-label">Loss</span>
                      <span className="training-stat-value" data-testid="training-loss">
                        {run.progress.loss.toFixed(2)}
                      </span>
                    </div>
                  ) : null}
                  {run.progress.eta_s !== null ? (
                    <div className="training-stat is-prominent">
                      <span className="training-stat-label">ETA</span>
                      <span className="training-stat-value" data-testid="training-eta">
                        {formatEta(run.progress.eta_s)}
                      </span>
                    </div>
                  ) : null}
                </>
              ) : (
                <div className="training-stat">
                  <span className="training-stat-label">State</span>
                  <span className="training-stat-value">running</span>
                </div>
              )}
            </div>
            <Button
              type="button"
              variant="destructive"
              disabled={busy}
              onClick={onStop}
            >
              Stop run
            </Button>
          </div>

          {run.progress ? (
            <div
              className="training-progress"
              role="progressbar"
              aria-label={`${run.kind} progress`}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={
                run.progress.total
                  ? Math.round((run.progress.step / run.progress.total) * 100)
                  : undefined
              }
            >
              <div
                className="training-progress-fill"
                style={{
                  width: run.progress.total
                    ? progressWidth(run.progress.step / run.progress.total)
                    : "100%",
                }}
              />
            </div>
          ) : null}

          {formatStarted(run.started) ? (
            <p className="training-muted">{formatStarted(run.started)}</p>
          ) : null}

          {run.log_tail.length ? (
            <ul className="training-log" data-testid="training-log">
              {run.log_tail.map((line, index) => (
                <li key={index}>{line}</li>
              ))}
            </ul>
          ) : null}
        </>
      ) : heroMode === "generation" && generation ? (
        <>
          <h3 className="training-hero-title" data-testid="training-state">
            {heroTitle}
          </h3>
          <div className="training-hero-body">
            <div className="training-hero-stats">
              {generation.target !== null ? (
                <div className="training-stat is-prominent">
                  <span className="training-stat-label">Rows</span>
                  <span className="training-stat-value" data-testid="training-generation-rows">
                    {`${generation.rows ?? 0} / ${generation.target}`}
                  </span>
                </div>
              ) : (
                <div className="training-stat is-prominent">
                  <span className="training-stat-label">Rows</span>
                  <span className="training-stat-value" data-testid="training-generation-rows">
                    {(generation.rows ?? 0)} rows
                  </span>
                </div>
              )}
              {generation.phase ? (
                <div className="training-stat">
                  <span className="training-stat-label">Phase</span>
                  <span className="training-stat-value">{generation.phase}</span>
                </div>
              ) : null}
            </div>
          </div>
          {generation.target !== null ? (
            <>
              <div
                className="training-progress"
                role="progressbar"
                aria-label="Data generation progress"
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={Math.round(
                  ((generation.rows ?? 0) / generation.target) * 100,
                )}
              >
                <div
                  className="training-progress-fill"
                  style={{
                    width: progressWidth((generation.rows ?? 0) / generation.target),
                  }}
                />
              </div>
            </>
          ) : null}
        </>
      ) : heroMode === "live" ? (
        <>
          <h3 className="training-hero-title" data-testid="training-state">
            {heroTitle}
          </h3>
          <div data-testid="training-live-state">
            <p className="training-muted">No progress reported by the host yet.</p>
            <div
              className="training-progress"
              role="progressbar"
              aria-label="Training in progress"
            />
          </div>
        </>
      ) : (
        <>
          <h3 className="training-hero-title" data-testid="training-state">
            {heroTitle}
          </h3>
          <p className="training-hero-sub">
            {runtimeUp
              ? `llama.cpp is serving${activeProfile ? ` · ${activeProfile}` : ""}.`
              : "Nothing is running. Start a job to begin."}
          </p>
          <div className="training-hero-body">
            <Button
              type="button"
              disabled={runRunning || busy}
              onClick={onTrain}
            >
              Unsloth train
            </Button>
            {runtimeUp ? (
              <Button type="button" variant="destructive" disabled={busy} onClick={onServeStop}>
                Stop serving
              </Button>
            ) : null}
          </div>
        </>
      )}

      {!run && !genRunning && state === "idle" ? (
        <p className="training-muted training-hero-note">No run in progress.</p>
      ) : null}
    </section>
  );
}
