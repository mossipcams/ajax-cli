import { useEffect, useState } from "react";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Button } from "@/shared/ui/button";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { useTrainingStatus } from "./useTrainingStatus";
import * as trainingApi from "./trainingApi";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const JOB_LABELS: Record<trainingApi.TrainingJobKind, string> = {
  generate: "Data generation",
  "lfm-train": "Unsloth train",
  "lfm-eval": "Unsloth eval",
  "wake-train": "Wake train",
};

type PendingAction =
  | { kind: "job"; job: trainingApi.TrainingJobKind }
  | { kind: "stop" }
  | { kind: "serve" }
  | { kind: "switch"; profile: string };

function formatEta(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  return minutes >= 1
    ? `~${minutes} min`
    : `~${Math.max(1, Math.round(seconds))}s`;
}

function progressWidth(fraction: number): string {
  const percent = Math.min(100, Math.max(0, fraction * 100));
  return `${Number(percent.toFixed(1))}%`;
}

/** Noun-first select-then-confirm sheet for the local training stack. */
export default function TrainingModal({ open, onOpenChange }: Props) {
  const { status, error, refresh } = useTrainingStatus(open);
  const [models, setModels] = useState<trainingApi.TrainingModels | null>(null);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void trainingApi
      .fetchTrainingModels()
      .then((result) => {
        if (cancelled) return;
        setModels(result);
      })
      .catch(() => {
        /* status banner surfaces failures; the models list simply stays empty */
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  useEffect(() => {
    if (!open) setPending(null);
  }, [open]);

  const run = status?.run ?? null;
  const generation = status?.generation ?? null;
  const details = status?.profile_details ?? null;
  const activeProfile =
    models?.active_profile ?? status?.active_profile ?? null;
  const runtimeUp = models?.running ?? status?.runtime_up === true;
  const busyReason =
    run?.running || (generation && generation.running)
      ? "a training job is in progress"
      : status && status.state.startsWith("train:")
        ? `state is ${status.state}`
        : null;

  async function execute(action: PendingAction): Promise<void> {
    setBusy(true);
    setActionError(null);
    try {
      if (action.kind === "job") await trainingApi.startTrainingJob(action.job);
      else if (action.kind === "stop") await trainingApi.stopTraining();
      else if (action.kind === "serve") await trainingApi.serveLlama();
      else await trainingApi.switchTrainingProfile(action.profile);
      setPending(null);
      void refresh();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  if (!open) return null;

  return (
    <FullscreenLayer zIndex={50}>
      <Sheet open onOpenChange={onOpenChange}>
        <SheetContent className="training-modal" aria-describedby={undefined}>
          <div className="training-header">
            <SheetTitle asChild>
              <h2>Training</h2>
            </SheetTitle>
            <button
              type="button"
              className="training-close"
              aria-label="Close"
              onClick={() => onOpenChange(false)}
            >
              ×
            </button>
          </div>

          {error ? (
            <p
              role="alert"
              className="training-banner"
              data-testid="training-status-error"
            >
              {error}
            </p>
          ) : null}

          {!status && !error ? (
            <p className="training-muted" data-testid="training-loading">
              Loading training status…
            </p>
          ) : null}

          <section
            className="training-section"
            aria-labelledby="training-models-heading"
          >
            <h3 id="training-models-heading">Models</h3>
            <ul className="training-profile-list">
              {(models?.profiles ?? []).map((name) => {
                const detail = details?.[name] ?? {};
                const isActive = name === activeProfile;
                return (
                  <li
                    key={name}
                    className={`training-profile${isActive ? " is-active" : ""}`}
                  >
                    <button
                      type="button"
                      className="training-profile-picker"
                      aria-pressed={isActive}
                      disabled={isActive || busy || Boolean(busyReason)}
                      onClick={() =>
                        void execute({ kind: "switch", profile: name })
                      }
                    >
                      <span className="training-profile-text">
                        <strong>{detail.label ?? name}</strong>
                        {isActive ? (
                          <em className="training-tag">active</em>
                        ) : null}
                        {isActive ? (
                          runtimeUp ? (
                            <em className="training-tag">serving</em>
                          ) : (
                            <em className="training-tag is-muted">stopped</em>
                          )
                        ) : null}
                        {detail.model ? (
                          <span className="training-model">{detail.model}</span>
                        ) : null}
                        {detail.serving ? (
                          <span className="training-serving">
                            {detail.serving}
                          </span>
                        ) : null}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
            {busyReason ? (
              <p className="training-muted">
                Switching is unavailable while {busyReason}.
              </p>
            ) : null}
            {activeProfile ? (
              runtimeUp ? (
                <Button
                  type="button"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void execute({ kind: "stop" })}
                >
                  {pending?.kind === "stop"
                    ? `Confirm stop ${activeProfile}`
                    : `Stop ${activeProfile}`}
                </Button>
              ) : pending?.kind === "serve" ? (
                <Button
                  type="button"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void execute(pending)}
                >
                  Confirm start {activeProfile}
                </Button>
              ) : (
                <Button
                  type="button"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => setPending({ kind: "serve" })}
                >
                  Start {activeProfile}
                </Button>
              )
            ) : null}
          </section>

          <section
            className="training-section"
            aria-labelledby="training-runs-heading"
          >
            <h3 id="training-runs-heading">Training runs</h3>
            {status ? (
              <p className="training-state" data-testid="training-state">
                {status.state}
              </p>
            ) : null}
            {run ? (
              <div className="training-run" data-testid="training-run">
                <p className="training-muted">{run.kind}</p>
                {run.progress ? (
                  <>
                    <div
                      className="training-progress"
                      role="progressbar"
                      aria-label={`${run.kind} progress`}
                      aria-valuemin={0}
                      aria-valuemax={100}
                      aria-valuenow={
                        run.progress.total
                          ? Math.round(
                              (run.progress.step / run.progress.total) * 100,
                            )
                          : undefined
                      }
                    >
                      <div
                        className="training-progress-fill"
                        style={{
                          width: run.progress.total
                            ? progressWidth(
                                run.progress.step / run.progress.total,
                              )
                            : "100%",
                        }}
                      />
                    </div>
                    <p className="training-muted">
                      step {run.progress.step}
                      {run.progress.total !== null
                        ? ` / ${run.progress.total}`
                        : ""}
                      {run.progress.loss !== null
                        ? ` · loss ${run.progress.loss.toFixed(4)}`
                        : ""}
                      {run.progress.eta_s !== null
                        ? ` · ETA ${formatEta(run.progress.eta_s)}`
                        : ""}
                    </p>
                  </>
                ) : (
                  <p className="training-muted">running</p>
                )}
                {run.log_tail.length ? (
                  <ul className="training-log" data-testid="training-log">
                    {run.log_tail.map((line, index) => (
                      <li key={index}>{line}</li>
                    ))}
                  </ul>
                ) : null}
              </div>
            ) : status ? (
              <p className="training-muted">No run in progress.</p>
            ) : null}
            <div className="training-actions">
              {(Object.keys(JOB_LABELS) as trainingApi.TrainingJobKind[]).map(
                (job) => {
                  const isPending =
                    pending?.kind === "job" && pending.job === job;
                  return isPending ? (
                    <Button
                      key={job}
                      type="button"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => void execute(pending)}
                    >
                      Confirm {JOB_LABELS[job]}
                    </Button>
                  ) : (
                    <Button
                      key={job}
                      type="button"
                      variant="secondary"
                      disabled={Boolean(run?.running) || busy}
                      onClick={() => setPending({ kind: "job", job })}
                    >
                      {JOB_LABELS[job]}
                    </Button>
                  );
                },
              )}
              {run?.running ? (
                pending?.kind === "stop" ? (
                  <Button
                    type="button"
                    disabled={busy}
                    onClick={() => void execute(pending)}
                  >
                    Confirm stop run
                  </Button>
                ) : (
                  <Button
                    type="button"
                    variant="destructive"
                    onClick={() => setPending({ kind: "stop" })}
                  >
                    Stop run
                  </Button>
                )
              ) : null}
            </div>
          </section>

          <section
            className="training-section"
            aria-labelledby="training-generation-heading"
          >
            <h3 id="training-generation-heading">Data generation</h3>
            {generation ? (
              generation.target !== null ? (
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
                        width: progressWidth(
                          (generation.rows ?? 0) / generation.target,
                        ),
                      }}
                    />
                  </div>
                  <p className="training-muted">
                    {generation.rows ?? 0} / {generation.target} rows
                    {generation.running ? " · generating" : ""}
                  </p>
                </>
              ) : (
                <p
                  className="training-muted"
                  data-testid="training-generation-rows"
                >
                  {generation.rows ?? 0} rows
                  {generation.running ? " · generating" : ""}
                </p>
              )
            ) : status ? (
              <p className="training-muted">No generation activity.</p>
            ) : null}
          </section>

          {actionError ? (
            <p
              role="alert"
              className="training-banner"
              data-testid="training-action-error"
            >
              {actionError}
            </p>
          ) : null}
        </SheetContent>
      </Sheet>
    </FullscreenLayer>
  );
}
