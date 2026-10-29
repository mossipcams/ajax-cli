import { Button } from "@/shared/ui/button";
import type { TrainingDataset, TrainingEvals, TrainingStatus } from "./trainingApi";
import type { PageAction } from "./trainingPageModel";
import TrainingFlowMap from "./TrainingFlowMap";
import { trainingFlowModel } from "./trainingFlowModel";

interface Props {
  status: TrainingStatus;
  championRunId: string | null;
  jobsDisabled: boolean;
  evals: TrainingEvals | null;
  datasets: TrainingDataset[] | null;
  onAction: (action: PageAction) => void;
}

export default function TrainingPipeline({ status, championRunId, jobsDisabled, evals, datasets, onAction }: Props) {
  const flow = trainingFlowModel(status, evals, datasets, jobsDisabled, championRunId);
  return (
    <section className="training-section" aria-labelledby="training-pipeline-heading" data-testid="training-pipeline">
      <h3 id="training-pipeline-heading">Pipeline</h3>
      <p className="training-muted">Training map · from candidate rows to a champion checkpoint</p>
      <TrainingFlowMap flow={flow} onAction={onAction} />
      <div className="training-actions-secondary training-page-secondary">
        <Button type="button" variant="secondary" disabled={jobsDisabled} onClick={() => onAction({ kind: "job", job: "generate" })}>
          Generate data
        </Button>
        <Button type="button" variant="secondary" disabled={jobsDisabled} onClick={() => onAction({ kind: "job", job: "wake-train" })}>
          Wake train
        </Button>
      </div>
    </section>
  );
}
