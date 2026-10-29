import type { ReactNode } from "react";
import { Button } from "@/shared/ui/button";
import { formatBytes } from "./TrainingDatasets";
import * as trainingApi from "./trainingApi";
import { formatEta, pathBasename, shortRunId, type PageAction } from "./trainingPageModel";

type StageState = "done" | "active" | "next" | "pending";

interface PipelineStep {
  label: string;
  job: trainingApi.TrainingJobKind | null;
  artifact: string | null;
  complete?: (pipeline: trainingApi.TrainingPipeline, championRunId: string | null) => boolean;
  running: (status: trainingApi.TrainingStatus, pipeline: trainingApi.TrainingPipeline) => boolean;
}

interface StageView extends PipelineStep {
  state: StageState;
}

const runningJob =
  (kind: string) =>
  (status: trainingApi.TrainingStatus): boolean =>
    status.run?.running === true && status.run.kind === kind;

const PIPELINE_STEPS: PipelineStep[] = [
  {
    label: "Generate",
    job: "generate",
    artifact: "generated rows",
    running: (status) => status.generation?.running === true || runningJob("generate")(status),
  },
  {
    label: "Validate",
    job: "validate",
    artifact: "validated run record",
    complete: (pipeline) => pipeline.validated,
    running: runningJob("validate"),
  },
  {
    label: "Canary",
    job: "canary",
    artifact: "canary-checked dataset",
    complete: (pipeline) => pipeline.canary_passed,
    running: runningJob("canary"),
  },
  {
    label: "Promote dataset",
    job: "promote-dataset",
    artifact: "promoted dataset",
    complete: (pipeline) => pipeline.dataset_promoted,
    running: runningJob("promote-dataset"),
  },
  {
    label: "Train",
    job: "lfm-train",
    artifact: "LoRA checkpoints",
    complete: (pipeline) => pipeline.trained,
    running: runningJob("lfm-train"),
  },
  {
    label: "Evaluate checkpoints",
    job: "select-checkpoint",
    artifact: "checkpoint scores",
    running: (status, pipeline) =>
      runningJob("select-checkpoint")(status) ||
      (status.eval_progress.length > 0 && pipeline.trained && !pipeline.final_eval_passed),
  },
  {
    label: "Eval",
    job: "lfm-eval",
    artifact: "eval scores vs champion",
    complete: (pipeline) => pipeline.final_eval_passed,
    running: runningJob("lfm-eval"),
  },
  {
    label: "Promote model",
    job: "promote-model",
    artifact: null,
    complete: (pipeline, championRunId) => championRunId === pipeline.run_id,
    running: runningJob("promote-model"),
  },
];

const STATE_TEXT: Record<StageState, string> = {
  done: "Done",
  active: "Running",
  next: "Next",
  pending: "Pending",
};

function stageStates(
  status: trainingApi.TrainingStatus,
  pipeline: trainingApi.TrainingPipeline,
  championRunId: string | null,
): StageView[] {
  let nextAssigned = false;
  return PIPELINE_STEPS.filter(
    (step) => step.job !== "select-checkpoint" || (status.checkpoint_eval && pipeline.trained),
  ).map((step) => {
    const complete = step.complete?.(pipeline, championRunId) ?? true;
    if (step.running(status, pipeline) && (!complete || step.complete === undefined)) {
      nextAssigned = true;
      return { ...step, state: "active" };
    }
    if (complete) return { ...step, state: "done" };
    if (!nextAssigned) {
      nextAssigned = true;
      return { ...step, state: "next" };
    }
    return { ...step, state: "pending" };
  });
}

function liveDetail(stage: StageView, status: trainingApi.TrainingStatus): ReactNode {
  if (stage.job === "generate") {
    const generation = status.generation;
    const rows = generation?.rows ?? 0;
    const target = generation?.target ?? null;
    return (
      <>
        <p className="training-muted">
          {target !== null ? `Rows ${rows} of ${target}` : `${rows} rows`}
          {generation?.phase ? ` · ${generation.phase}` : ""}
        </p>
        <progress
          className="training-page-progress"
          aria-label="Generate stage progress"
          {...(target ? { value: rows, max: target } : {})}
        />
      </>
    );
  }
  if (stage.job === "lfm-train") {
    const progress = status.run?.progress ?? null;
    const total = progress?.total ?? null;
    return (
      <>
        <p className="training-muted">
          {progress
            ? total
              ? `Step ${progress.step} of ${total}`
              : `Step ${progress.step}`
            : "Waiting for progress"}
          {progress?.loss != null ? ` · loss ${progress.loss.toFixed(4)}` : ""}
          {progress?.eta_s != null ? ` · ETA ${formatEta(progress.eta_s)}` : ""}
        </p>
        <progress
          className="training-page-progress"
          aria-label="Train stage progress"
          {...(progress && total ? { value: progress.step, max: total } : {})}
        />
      </>
    );
  }
  if (stage.job === "select-checkpoint" || stage.job === "lfm-eval") {
    const evalProgress = status.eval_progress[0] ?? null;
    if (!evalProgress) return <p className="training-muted">Running</p>;
    const total = evalProgress.labels.length;
    const done = evalProgress.done.length;
    const current = evalProgress.current ? `: ${evalProgress.current}` : "";
    return (
      <>
        <p className="training-muted">{`Suite ${Math.min(done + 1, total)} of ${total}${current}`}</p>
        <progress
          className="training-page-progress"
          aria-label={`${stage.label} stage progress`}
          value={done}
          max={total}
        />
      </>
    );
  }
  return null;
}

function finalEvalSummary(runId: string, evals: trainingApi.TrainingEvals | null): string | null {
  const latest = (evals?.evals ?? [])
    .filter((row) => row.kind === "final" && row.group.startsWith(runId) && row.pass_rate !== null)
    .sort((a, b) => (b.created_at ?? "").localeCompare(a.created_at ?? ""))[0];
  if (!latest || latest.pass_rate === null) return null;
  const pass = `Pass ${(latest.pass_rate * 100).toFixed(1)}%`;
  const championRate = latest.suite ? evals?.champion?.pass_rates[latest.suite] : undefined;
  if (championRate === undefined) return pass;
  const delta = (latest.pass_rate - championRate) * 100;
  return `${pass} · ${delta >= 0 ? "+" : "-"}${Math.abs(delta).toFixed(1)} pts vs champion`;
}

function resultDetail(
  stage: StageView,
  pipeline: trainingApi.TrainingPipeline,
  evals: trainingApi.TrainingEvals | null,
  datasets: trainingApi.TrainingDataset[] | null,
): string | null {
  if (stage.job === "generate") {
    const current = datasets?.find((dataset) => dataset.current) ?? null;
    const runId = shortRunId(current?.run_id ?? pipeline.run_id);
    return current?.size_bytes != null
      ? `Run ${runId} · ${formatBytes(current.size_bytes)}`
      : `Run ${runId}`;
  }
  if (stage.job === "lfm-eval") return finalEvalSummary(pipeline.run_id, evals);
  if (stage.job === "promote-model") {
    const champion = evals?.champion ?? null;
    return champion && champion.run_id === pipeline.run_id && champion.checkpoint
      ? `Champion ${pathBasename(champion.checkpoint)}`
      : null;
  }
  return null;
}

interface FlowProps {
  status: trainingApi.TrainingStatus;
  pipeline: trainingApi.TrainingPipeline;
  championRunId: string | null;
  evals: trainingApi.TrainingEvals | null;
  datasets: trainingApi.TrainingDataset[] | null;
  jobsDisabled: boolean;
  promotedRunId: string | null;
  promotedRecordPresent: boolean;
  onAction: (action: PageAction) => void;
}

function PipelineFlow({
  status,
  pipeline,
  championRunId,
  evals,
  datasets,
  jobsDisabled,
  promotedRunId,
  promotedRecordPresent,
  onAction,
}: FlowProps) {
  const stages = stageStates(status, pipeline, championRunId);
  const activeIndex = stages.findIndex((stage) => stage.state === "active");
  const focus = activeIndex >= 0 ? activeIndex : stages.findIndex((stage) => stage.state === "next");
  const trainStartable = promotedRunId !== null && promotedRecordPresent && !pipeline.trained;

  return (
    <>
      <div className="training-flow-header">
        <p className="training-flow-position" data-testid="training-flow-position">
          {`Stage ${focus >= 0 ? focus + 1 : stages.length} of ${stages.length} · ${focus >= 0 ? stages[focus].label : "Complete"}`}
        </p>
        <div className="training-flow-segments" aria-hidden="true">
          {stages.map((stage) => (
            <span key={stage.label} className={`training-flow-segment is-${stage.state}`} />
          ))}
        </div>
      </div>
      <ol className="training-page-steps">
        {stages.map((stage, index) => {
          const below: StageView | undefined = stages[index + 1];
          const isTrainStep = stage.job === "lfm-train";
          const showJobStart =
            stage.job !== null &&
            (stage.state === "next" ||
              (stage.job === "select-checkpoint" && status.checkpoint_eval && pipeline.trained));
          const jobDisabled = jobsDisabled || (isTrainStep && (!trainStartable || pipeline.trained));
          const flow =
            stage.state === "done"
              ? below?.state === "active"
                ? "flowing"
                : "done"
              : "idle";
          const detail = stage.state === "done" ? resultDetail(stage, pipeline, evals, datasets) : null;
          return (
            <li
              key={stage.label}
              className={`training-page-step is-${stage.state}`}
              data-testid="training-pipeline-step"
              data-state={stage.state}
            >
              <span className="training-flow-dot" aria-hidden="true" />
              <div className="training-flow-body">
                <div className="training-page-step-head">
                  <span className="training-page-step-label">{stage.label}</span>
                  {isTrainStep ? (
                    <span className="training-muted" data-testid="training-train-hint">
                      {promotedRunId === null
                        ? "Promote a dataset first"
                        : `Trains ${shortRunId(promotedRunId)}`}
                    </span>
                  ) : (
                    <span className="training-muted">{STATE_TEXT[stage.state]}</span>
                  )}
                  {showJobStart ? (
                    <Button
                      type="button"
                      disabled={jobDisabled}
                      onClick={() =>
                        onAction({
                          kind: "job",
                          job: stage.job as trainingApi.TrainingJobKind,
                        })
                      }
                    >
                      Start {stage.label.toLowerCase()}
                    </Button>
                  ) : null}
                </div>
                {stage.state === "active" ? liveDetail(stage, status) : null}
                {detail ? <p className="training-muted">{detail}</p> : null}
              </div>
              {below ? (
                <>
                  <span className={`training-flow-rail is-${flow}`} aria-hidden="true" />
                  <div className={`training-flow-link is-${flow}`} data-testid="training-flow-link" data-flow={flow}>
                    <span className="training-flow-artifact">{stage.artifact}</span>
                  </div>
                </>
              ) : null}
            </li>
          );
        })}
      </ol>
    </>
  );
}

interface Props {
  status: trainingApi.TrainingStatus;
  championRunId: string | null;
  jobsDisabled: boolean;
  evals: trainingApi.TrainingEvals | null;
  datasets: trainingApi.TrainingDataset[] | null;
  onAction: (action: PageAction) => void;
}

export default function TrainingPipeline({
  status,
  championRunId,
  jobsDisabled,
  evals,
  datasets,
  onAction,
}: Props) {
  const pipeline = status.pipeline;
  const promotedRunId = pipeline?.promoted_run_id ?? null;
  const promotedRecordPresent = pipeline?.promoted_record_present === true;
  const trainUsesDifferentDataset =
    promotedRunId !== null && pipeline !== null && promotedRunId !== pipeline.run_id;

  return (
    <section className="training-section" aria-labelledby="training-pipeline-heading" data-testid="training-pipeline">
      <h3 id="training-pipeline-heading">Pipeline</h3>
      {pipeline ? (
        <>
          <p className="training-muted">
            Dataset <code>{pipeline.run_id}</code>
          </p>
          {trainUsesDifferentDataset ? (
            <p
              className="training-banner"
              role="note"
              data-testid="training-train-promoted-note"
            >
              Train uses promoted dataset {promotedRunId}, not the selected one
            </p>
          ) : null}
          {promotedRunId !== null && !promotedRecordPresent ? (
            <p
              role="alert"
              className="training-banner"
              data-testid="training-train-missing-promoted"
            >
              Promoted dataset {promotedRunId} is missing from this checkout — Train will fail
            </p>
          ) : null}
          <PipelineFlow
            status={status}
            pipeline={pipeline}
            championRunId={championRunId}
            evals={evals}
            datasets={datasets}
            jobsDisabled={jobsDisabled}
            promotedRunId={promotedRunId}
            promotedRecordPresent={promotedRecordPresent}
            onAction={onAction}
          />
        </>
      ) : (
        <p className="training-muted">No dataset pipeline for the current run yet.</p>
      )}

      <div className="training-actions-secondary training-page-secondary">
        <Button
          type="button"
          variant="secondary"
          disabled={jobsDisabled}
          onClick={() => onAction({ kind: "job", job: "generate" })}
        >
          Generate data
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={jobsDisabled}
          onClick={() => onAction({ kind: "job", job: "wake-train" })}
        >
          Wake train
        </Button>
      </div>
    </section>
  );
}
