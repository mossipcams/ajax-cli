import { useEffect, useRef, useState } from "react";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { trainingHash } from "@/shared/lib/routes";
import { markNavigationStart } from "@/shared/lib/telemetry";
import { Button } from "@/shared/ui/button";
import Skeleton from "@/shared/ui/Skeleton";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { useTrainingStatus } from "./useTrainingStatus";
import TrainingProfileList from "./TrainingProfileList";
import { formatElapsed, jobLabel } from "./trainingPageModel";
import * as trainingApi from "./trainingApi";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

type PendingAction =
  | { kind: "serve" }
  | { kind: "serve-stop" }
  | { kind: "switch"; profile: string };


function confirmDescription(action: PendingAction): string {
  switch (action.kind) {
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
      if (action.kind === "serve-stop") await trainingApi.stopTraining();
      else if (action.kind === "serve") await trainingApi.serveLlama();
      else await trainingApi.switchTrainingProfile(action.profile);
      setPending(null);
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
  const progress = run?.progress ?? null;
  const elapsed = run ? formatElapsed(run.started) : null;

  return (
    <FullscreenLayer zIndex={50}>
      <Sheet open onOpenChange={onOpenChange}>
        <SheetContent className="training-modal" aria-describedby={undefined} data-testid="training">
          <div className="training-header">
            <SheetTitle asChild>
              <h2>Local</h2>
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

              {run?.running ? (
                <section
                  className="training-local-run"
                  aria-label="Training progress"
                  data-testid="training-local-run"
                >
                  <p className="training-local-run-label">{jobLabel(run.kind)}</p>
                  {elapsed ? (
                    <p className="training-local-run-text">{elapsed} elapsed</p>
                  ) : null}
                  <p className="training-local-run-text">
                    {progress
                      ? progress.total !== null
                        ? `Step ${progress.step} / ${progress.total}`
                        : `Step ${progress.step}`
                      : "Waiting for progress"}
                  </p>
                  {progress && progress.total !== null ? (
                    <progress
                      value={progress.step}
                      max={progress.total}
                      aria-label="Training step progress"
                    />
                  ) : (
                    <progress aria-label="Training step progress" />
                  )}
                </section>
              ) : null}

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
