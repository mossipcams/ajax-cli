import { useEffect, useRef, useState } from "react";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { trainingHash } from "@/shared/lib/routes";
import { markNavigationStart } from "@/shared/lib/telemetry";
import { Button } from "@/shared/ui/button";
import Skeleton from "@/shared/ui/Skeleton";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { useTrainingStatus } from "./useTrainingStatus";
import TrainingProfileList from "./TrainingProfileList";
import TrainingStatusPanel from "./TrainingStatusPanel";
import * as trainingApi from "./trainingApi";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const JOB_LABELS = trainingApi.TRAINING_JOB_LABELS;

// Jobs that sit behind the primary Unsloth train action.
const SECONDARY_JOBS: trainingApi.TrainingJobKind[] = [
  "generate",
  "lfm-eval",
  "wake-train",
];

type PendingAction =
  | { kind: "job"; job: trainingApi.TrainingJobKind }
  | { kind: "stop" }
  | { kind: "serve" }
  | { kind: "serve-stop" }
  | { kind: "switch"; profile: string };


function confirmDescription(action: PendingAction): string {
  switch (action.kind) {
    case "job":
      return `Start the ${JOB_LABELS[action.job]} job?`;
    case "stop":
      return "Stop the current training run?";
    case "serve":
      return "Start llama.cpp serving for the active profile?";
    case "serve-stop":
      return "Stop llama.cpp serving?";
    case "switch":
      return `Switch the active profile to ${action.profile}?`;
  }
}

function busyText(action: PendingAction): string {
  switch (action.kind) {
    case "job":
      return `Starting ${JOB_LABELS[action.job]}`;
    case "stop":
      return "Stopping run";
    case "serve":
      return "Starting serving";
    case "serve-stop":
      return "Stopping serving";
    case "switch":
      return `Switching to ${action.profile}`;
  }
}

export default function TrainingModal({ open, onOpenChange }: Props) {
  const { status, error, refresh } = useTrainingStatus(open);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [active, setActive] = useState<PendingAction | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  // Mirrors `open` for event handlers: a POST that rejects after the user
  // closed the dialog must not surface a stale error banner on next open.
  const openRef = useRef(open);
  const actionInFlightRef = useRef(false);
  useEffect(() => {
    openRef.current = open;
  }, [open]);

  useEffect(() => {
    if (!open) {
      setPending(null);
      setActive(null);
      setActionError(null);
    }
  }, [open]);

  const run = status?.run ?? null;
  const generation = status?.generation ?? null;
  const trainLabel = status?.state.startsWith("train:")
    ? status.state.slice("train:".length).split(" ")[0]
    : null;
  const details = status?.profile_details ?? null;
  const profiles = status?.profiles ?? [];
  const activeProfile = status?.active_profile ?? null;
  const runtimeUp = status?.runtime_up === true;
  const busy = active !== null;
  const runRunning = Boolean(run?.running);
  const genRunning = Boolean(generation && generation.running);
  const heroMode: "run" | "generation" | "live" | "idle" = runRunning
    ? "run"
    : genRunning
      ? "generation"
      : trainLabel
        ? "live"
        : "idle";

  function armJob(job: trainingApi.TrainingJobKind): void {
    setPending({ kind: "job", job });
  }

  function armStop(): void {
    setPending({ kind: "stop" });
  }

  function armServe(): void {
    setPending({ kind: "serve" });
  }

  function armServeStop(): void {
    setPending({ kind: "serve-stop" });
  }

  function armSwitch(profile: string): void {
    setPending({ kind: "switch", profile });
  }

  function cancelPending(): void {
    setPending(null);
  }

  function confirmPending(): void {
    if (pending) void execute(pending);
  }

  async function execute(action: PendingAction): Promise<void> {
    if (actionInFlightRef.current) return;
    actionInFlightRef.current = true;
    setActive(action);
    setActionError(null);
    try {
      if (action.kind === "job") await trainingApi.startTrainingJob(action.job);
      else if (action.kind === "stop" || action.kind === "serve-stop")
        await trainingApi.stopTraining();
      else if (action.kind === "serve") await trainingApi.serveLlama();
      else await trainingApi.switchTrainingProfile(action.profile);
      setPending(null);
      // Skip the refetch if the dialog closed while the action completed;
      // reopening reloads fresh state anyway (same guard as the error path).
      if (openRef.current) void refresh();
    } catch (err) {
      if (openRef.current) {
        setActionError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      actionInFlightRef.current = false;
      setActive(null);
    }
  }

  function handleClose(): void {
    onOpenChange(false);
  }

  function openTrainingPage(): void {
    markNavigationStart(undefined, "hash");
    onOpenChange(false);
    window.location.hash = trainingHash();
  }

  if (!open) return null;

  const switchBlocked =
    runRunning || genRunning ? "a job is in progress" : trainLabel ? `state is ${status?.state}` : null;
  const confirmInFlight = busy && active !== null && pending === active;

  const heroTitle =
    heroMode === "run" && run
      ? `${JOB_LABELS[run.kind as trainingApi.TrainingJobKind] ?? run.kind} running`
      : heroMode === "generation"
        ? "Data generation running"
        : heroMode === "live"
          ? `Training in progress: ${trainLabel}`
          : runtimeUp
            ? "Runtime ready"
            : "Idle";

  return (
    <FullscreenLayer zIndex={50}>
      <Sheet open onOpenChange={onOpenChange}>
        <SheetContent className="training-modal" aria-describedby={undefined} data-testid="training">
          <div className="training-header">
            <SheetTitle asChild>
              <h2>Training</h2>
            </SheetTitle>
            <button
              type="button"
              className="training-close"
              aria-label="Close"
              onClick={handleClose}
            >
              ×
            </button>
          </div>

          {error && !status ? (
            <p
              role="alert"
              className="training-banner"
              data-testid="training-status-error"
            >
              {error}
            </p>
          ) : null}

          {!status && !error ? (
            <Skeleton rows={4} testid="training-loading" />
          ) : null}

          {status ? (
            <>
              <Button
                type="button"
                variant="secondary"
                className="training-page-link"
                onClick={openTrainingPage}
              >
                Training
              </Button>

              <TrainingStatusPanel
                heroMode={heroMode}
                heroTitle={heroTitle}
                state={status.state}
                run={run}
                generation={generation}
                runtimeUp={runtimeUp}
                activeProfile={activeProfile}
                busy={busy}
                runRunning={runRunning}
                genRunning={genRunning}
                onStop={armStop}
                onServeStop={armServeStop}
                onTrain={() => armJob("lfm-train")}
              />

              {/* Finished-but-recent run summary (run object present, not running). */}
              {run && !run.running ? (
                <div className="training-run" data-testid="training-run-idle">
                  <p className="training-muted">
                    {JOB_LABELS[run.kind as trainingApi.TrainingJobKind] ?? run.kind} finished
                  </p>
                </div>
              ) : null}

              {/* Actions: primary train job, secondary jobs grouped. */}
              <section className="training-section" aria-labelledby="training-jobs-heading">
                <h3 id="training-jobs-heading">Jobs</h3>
<Button
                  type="button"
                  className="training-actions-primary"
                  disabled={runRunning || busy}
                  onClick={() => armJob("lfm-train")}
                >
                  Unsloth train
                </Button>
                <div className="training-actions-secondary">
                  {SECONDARY_JOBS.map((job) => (
                    <Button
                      key={job}
                      type="button"
                      variant="secondary"
                      disabled={runRunning || busy}
                      onClick={() => armJob(job)}
                    >
                      {JOB_LABELS[job]}
                    </Button>
                  ))}
                </div>
              </section>

              <TrainingProfileList
                runtimeUp={runtimeUp}
                activeProfile={activeProfile}
                busy={busy}
                switchBlocked={switchBlocked}
                switchingKind={active?.kind === "switch" ? "switch" : null}
                switchingProfile={active?.kind === "switch" ? active.profile : null}
                profiles={profiles}
                details={details}
                profilesError={error}
                onServe={armServe}
                onServeStop={armServeStop}
                onSwitch={armSwitch}
              />

              {/* One consistent confirmation bar for every mutating action. */}
              {pending ? (
                <div className="training-confirm" data-testid="training-confirm">
                  <p className="training-confirm-text">{confirmDescription(pending)}</p>
                  <div className="training-confirm-buttons">
                    <Button
                      type="button"
                      disabled={active !== null}
                      aria-busy={confirmInFlight || undefined}
                      onClick={confirmPending}
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
                    <Button type="button" variant="secondary" disabled={busy} onClick={cancelPending}>
                      Cancel
                    </Button>
                  </div>
                </div>
              ) : null}

              {actionError ? (
                <p role="alert" className="training-banner" data-testid="training-action-error">
                  {actionError}
                </p>
              ) : null}
            </>
          ) : null}
        </SheetContent>
      </Sheet>
    </FullscreenLayer>
  );
}
