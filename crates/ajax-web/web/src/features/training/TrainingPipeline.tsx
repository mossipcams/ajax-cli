import { Button } from "@/shared/ui/button";
import * as trainingApi from "./trainingApi";
import { shortRunId, type PageAction } from "./trainingPageModel";

interface PipelineStep {
  label: string;
  job: trainingApi.TrainingJobKind | null;
  isDone: (pipeline: trainingApi.TrainingPipeline, championRunId: string | null) => boolean;
}

const PIPELINE_STEPS: PipelineStep[] = [
  { label: "Generate", job: null, isDone: () => true },
  { label: "Validate", job: "validate", isDone: (pipeline) => pipeline.validated },
  { label: "Canary", job: "canary", isDone: (pipeline) => pipeline.canary_passed },
  {
    label: "Promote dataset",
    job: "promote-dataset",
    isDone: (pipeline) => pipeline.dataset_promoted,
  },
  { label: "Train", job: "lfm-train", isDone: (pipeline) => pipeline.trained },
  {
    label: "Evaluate checkpoints",
    job: "select-checkpoint",
    isDone: () => true,
  },
  { label: "Eval", job: "lfm-eval", isDone: (pipeline) => pipeline.final_eval_passed },
  {
    label: "Promote model",
    job: "promote-model",
    isDone: (pipeline, championRunId) => championRunId === pipeline.run_id,
  },
];

function pipelineStepsFor(
  pipeline: trainingApi.TrainingPipeline,
  checkpointEval: boolean,
): PipelineStep[] {
  return PIPELINE_STEPS.filter(
    (step) =>
      step.job !== "select-checkpoint" || (checkpointEval && pipeline.trained),
  );
}

function stepStates(
  pipeline: trainingApi.TrainingPipeline,
  championRunId: string | null,
  checkpointEval: boolean,
): Array<PipelineStep & { state: "done" | "next" | "pending" }> {
  let nextAssigned = false;
  return pipelineStepsFor(pipeline, checkpointEval).map((step) => {
    if (step.isDone(pipeline, championRunId)) return { ...step, state: "done" };
    if (!nextAssigned) {
      nextAssigned = true;
      return { ...step, state: "next" };
    }
    return { ...step, state: "pending" };
  });
}

interface Props {
  status: trainingApi.TrainingStatus;
  championRunId: string | null;
  jobsDisabled: boolean;
  onAction: (action: PageAction) => void;
}

export default function TrainingPipeline({ status, championRunId, jobsDisabled, onAction }: Props) {
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
          <ol className="training-page-steps">
            {stepStates(pipeline, championRunId, status.checkpoint_eval).map((step) => {
              const isTrainStep = step.job === "lfm-train";
              const trainStartable =
                promotedRunId !== null && promotedRecordPresent && !pipeline.trained;
              const showJobStart =
                step.job !== null &&
                (step.state === "next" ||
                  (step.job === "select-checkpoint" &&
                    status.checkpoint_eval &&
                    pipeline.trained));
              const jobDisabled =
                jobsDisabled || (isTrainStep && (!trainStartable || pipeline.trained));
              return (
                <li
                  key={step.label}
                  className={`training-page-step is-${step.state}`}
                  data-testid="training-pipeline-step"
                  data-state={step.state}
                >
                  <span className="training-page-step-label">{step.label}</span>
                  {isTrainStep ? (
                    <span className="training-muted" data-testid="training-train-hint">
                      {promotedRunId === null
                        ? "Promote a dataset first"
                        : `Trains ${shortRunId(promotedRunId)}`}
                    </span>
                  ) : (
                    <span className="training-muted">
                      {step.state === "done"
                        ? "Done"
                        : step.state === "next"
                          ? "Next"
                          : "Pending"}
                    </span>
                  )}
                  {showJobStart ? (
                    <Button
                      type="button"
                      disabled={jobDisabled}
                      onClick={() =>
                        onAction({
                          kind: "job",
                          job: step.job as trainingApi.TrainingJobKind,
                        })
                      }
                    >
                      Start {step.label.toLowerCase()}
                    </Button>
                  ) : null}
                </li>
              );
            })}
          </ol>
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
