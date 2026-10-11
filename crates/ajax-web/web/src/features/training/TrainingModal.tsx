import { useEffect, useRef, useState } from "react";
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
  | { kind: "switch"; profile: string }
  | { kind: "serve-stop" };

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

function formatStarted(started: string | null): string {
  if (!started) return "";
  const startedMs = Date.parse(started);
  if (!Number.isFinite(startedMs)) return "";
  const text = new Date(startedMs).toLocaleString();
  let elapsed = "";
  const diff = Math.max(0, Date.now() - startedMs);
  if (diff > 5000) {
    const seconds = Math.floor(diff / 1000);
    if (seconds < 60) elapsed = `${seconds} s`;
    else if (seconds < 3600) elapsed = `${Math.floor(seconds / 60)} min`;
    else elapsed = `${Math.floor(seconds / 3600)} h ${Math.floor((seconds % 3600) / 60)} min`;
  }
  return elapsed ? `${text} · ${elapsed}` : text;
}

function statusLine(state: string, runningKind: string | null): string {
  if (runningKind) {
    return (JOB_LABELS as Record<string, string>)[runningKind] ?? runningKind;
  }
  if (state.startsWith("train:")) return "Training in progress";
  if (state.startsWith("generate")) return "Generating data";
  if (state === "idle") return "Idle";
  return state;
}

export default function TrainingModal({ open, onOpenChange }: Props) {
  const { status, error, refresh } = useTrainingStatus(open);
  const [models, setModels] = useState<trainingApi.TrainingModels | null>(null);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [active, setActive] = useState<PendingAction | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [profilesError, setProfilesError] = useState<string | null>(null);
  // Mirrors `open` for event handlers: a POST that rejects after the user
  // closed the dialog must not surface a stale error banner on next open.
  const openRef = useRef(open);
  useEffect(() => {
    openRef.current = open;
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void trainingApi
      .fetchTrainingModels()
      .then((result) => {
        if (cancelled) return;
        setModels(result);
      })
      .catch((err: unknown) => {
        if (!cancelled) setProfilesError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  useEffect(() => {
    if (!open) {
      setPending(null);
      setActive(null);
      setActionError(null);
      setProfilesError(null);
    }
  }, [open]);

  const run = status?.run ?? null;
  const generation = status?.generation ?? null;
  const trainLabel = status?.state.startsWith("train:")
    ? status.state.slice("train:".length).split(" ")[0]
    : null;
  const details = status?.profile_details ?? null;
  const activeProfile =
    models?.active_profile ?? status?.active_profile ?? null;
  const runtimeUp = models?.running ?? status?.runtime_up === true;
  const busy = active !== null;
  const busyReason =
    run?.running || (generation && generation.running)
      ? "a training job is in progress"
      : status && status.state.startsWith("train:")
        ? `state is ${status.state}`
        : null;

  async function execute(action: PendingAction): Promise<void> {
    setActive(action);
    setActionError(null);
    try {
      if (action.kind === "job") await trainingApi.startTrainingJob(action.job);
      else if (action.kind === "stop" || action.kind === "serve-stop")
        await trainingApi.stopTraining();
      else if (action.kind === "serve") await trainingApi.serveLlama();
      else await trainingApi.switchTrainingProfile(action.profile);
      setPending(null);
      void refresh();
    } catch (err) {
      if (openRef.current) {
        setActionError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      setActive(null);
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
            aria-labelledby="training-runs-heading"
          >
            <h3 id="training-runs-heading">Training runs</h3>
            {status ? (
              <>
                <p className="training-state" data-testid="training-state">
                  {statusLine(status.state, run?.running ? run.kind : null)}
                </p>
                <p
                  className="training-muted training-raw-state"
                  data-testid="training-raw-state"
                >
                  {status.state}
                </p>
              </>
            ) : null}
            {run ? (
              <div className="training-run" data-testid={run.running ? "training-run" : "training-run-idle"}>
                <p className="training-muted">{run.kind}</p>
                {!run.running && (
                  <p className="training-muted">Run finished</p>
                )}
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

                {formatStarted(run.started) ? (
                  <p className="training-muted">{formatStarted(run.started)}</p>
                ) : null}
              </div>
            ) : trainLabel ? (
              <div data-testid="training-live-state">
                <p className="training-muted">
                  Training in progress: {trainLabel}
                </p>
                <div
                  className="training-progress"
                  role="progressbar"
                  aria-label="Training in progress"
                />
              </div>
            ) : status ? (
              <p className="training-muted">No run in progress.</p>
            ) : null}
            <div className="training-actions">
              {(Object.keys(JOB_LABELS) as trainingApi.TrainingJobKind[]).map(
                (job) => {
                  const isPending =
                    pending?.kind === "job" && pending.job === job;
                  const inFlight = active?.kind === "job" && active.job === job;
                  return isPending ? (
                    <Button
                      key={job}
                      type="button"
                      variant="secondary"
                      disabled={busy}
                      aria-busy={inFlight || undefined}
                      onClick={() => void execute(pending)}
                    >
                      {inFlight ? (
                        <>
                          <span className="training-busy-dot" aria-hidden />
                          Starting {JOB_LABELS[job]}…
                        </>
                      ) : (
                        `Confirm ${JOB_LABELS[job]}`
                      )}
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
                    aria-busy={active?.kind === "stop" || undefined}
                    onClick={() => void execute(pending)}
                  >
                    {active?.kind === "stop" ? (
                      <>
                        <span className="training-busy-dot" aria-hidden />
                        Stopping run…
                      </>
                    ) : (
                      "Confirm stop run"
                    )}
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
            aria-labelledby="training-models-heading"
          >
            <h3 id="training-models-heading">llama.cpp</h3>
            <p className="training-muted" data-testid="llama-state">
              {runtimeUp ? "Serving" : "Stopped"}
              {activeProfile ? ` · ${activeProfile}` : ""}
            </p>
            <div className="training-llama-actions">
              <Button
                type="button"
                disabled={busy || runtimeUp || !activeProfile}
                aria-busy={active?.kind === "serve" || undefined}
                onClick={() => void execute({ kind: "serve" })}
              >
                {active?.kind === "serve" ? (
                  <>
                    <span className="training-busy-dot" aria-hidden />
                    Starting…
                  </>
                ) : (
                  "Start"
                )}
              </Button>
              <Button
                type="button"
                variant="destructive"
                disabled={busy || !runtimeUp}
                aria-busy={active?.kind === "serve-stop" || undefined}
                onClick={() => void execute({ kind: "serve-stop" })}
              >
                {active?.kind === "serve-stop" ? (
                  <>
                    <span className="training-busy-dot" aria-hidden />
                    Stopping…
                  </>
                ) : (
                  "Stop"
                )}
              </Button>
            </div>
            <ul className="training-profile-list">
              {(models?.profiles ?? []).map((name) => {
                const detail = details?.[name] ?? {};
                const isActive = name === activeProfile;
                const isArmed = pending?.kind === "switch" && pending.profile === name;
                const switching = active?.kind === "switch" && active.profile === name;
                return (
                  <li
                    key={name}
                    className={`training-profile${isActive ? " is-active" : ""}`}
                  >
                    {switching ? (
                      <p className="training-muted" data-testid="training-switch-busy">
                        Switching to {detail.label ?? name}…
                      </p>
                    ) : isArmed ? (
                      <div className="training-llama-actions">
                        <Button
                          type="button"
                          variant="secondary"
                          disabled={busy || isActive}
                          onClick={() => void execute(pending)}
                        >
                          Confirm switch to {detail.label ?? name}
                        </Button>
                        <Button
                          type="button"
                          disabled={busy}
                          onClick={() => setPending(null)}
                        >
                          Cancel
                        </Button>
                      </div>
                    ) : (
                    <button
                      type="button"
                      className="training-profile-picker"
                      aria-pressed={isActive}
                      disabled={isActive || busy || Boolean(busyReason)}
                      onClick={() => setPending({ kind: "switch", profile: name })}
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
                    )}
                  </li>
                );
              })}
            </ul>

            {profilesError ? (
              <p
                role="alert"
                className="training-banner"
                data-testid="training-profiles-error"
              >
                Model list unavailable: {profilesError}
              </p>
            ) : null}
            {busyReason ? (
              <p className="training-muted">
                Switching is unavailable while {busyReason}.
              </p>
            ) : null}
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
                    {generation.phase ? `${generation.phase}: ` : ""}
                    {generation.rows ?? 0} / {generation.target}
                    {generation.phase ? "" : " rows"}
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
