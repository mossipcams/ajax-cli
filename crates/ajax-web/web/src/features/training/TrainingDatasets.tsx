import { Button } from "@/shared/ui/button";
import Skeleton from "@/shared/ui/Skeleton";
import type { TrainingDataset } from "./trainingApi";
import type { PageAction } from "./trainingPageModel";

const STAGE_TAGS: Array<{
  key: "validated" | "canary_passed" | "dataset_promoted" | "trained" | "final_eval_passed";
  label: string;
}> = [
  { key: "validated", label: "Validated" },
  { key: "canary_passed", label: "Canary passed" },
  { key: "dataset_promoted", label: "Dataset promoted" },
  { key: "trained", label: "Trained" },
  { key: "final_eval_passed", label: "Eval passed" },
];

function formatBytes(bytes: number | null): string {
  if (bytes === null) return "size unknown";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`;
}

interface Props {
  datasets: TrainingDataset[] | null;
  promotedRunId: string | null;
  selectDisabled: boolean;
  onAction: (action: PageAction) => void;
}

export default function TrainingDatasets({ datasets, promotedRunId, selectDisabled, onAction }: Props) {
  const missingPromotedInCatalog =
    promotedRunId !== null && datasets !== null && !datasets.some((dataset) => dataset.promoted);

  return (
    <section className="training-section" aria-labelledby="training-datasets-heading">
      <h3 id="training-datasets-heading">Datasets</h3>
      {datasets === null ? (
        <Skeleton rows={2} testid="training-datasets-loading" />
      ) : datasets.length === 0 ? (
        <p className="training-muted">No datasets yet.</p>
      ) : (
        <>
        {missingPromotedInCatalog ? (
          <p role="alert" className="training-banner" data-testid="training-datasets-promoted-missing">
            Promoted dataset {promotedRunId} is not listed in this checkout.
          </p>
        ) : null}
        <ul className="training-page-list">
          {datasets.map((dataset) => (
            <li key={dataset.run_id} className="training-page-row" data-testid="training-dataset-row">
              <div className="training-page-row-head">
                <code className="training-page-runid">{dataset.run_id}</code>
                <span className="training-muted">{formatBytes(dataset.size_bytes)}</span>
              </div>
              <div className="training-page-badges">
                {dataset.current ? <span className="training-tag">current</span> : null}
                {dataset.promoted ? <span className="training-tag">promoted</span> : null}
                {STAGE_TAGS.filter((stage) => dataset[stage.key]).map((stage) => (
                  <span key={stage.key} className="training-tag is-muted">
                    {stage.label}
                  </span>
                ))}
              </div>
              {dataset.current ? null : (
                <Button
                  type="button"
                  variant="secondary"
                  disabled={selectDisabled}
                  onClick={() => onAction({ kind: "select", runId: dataset.run_id })}
                >
                  Select
                </Button>
              )}
            </li>
          ))}
        </ul>
        </>
      )}
    </section>

  );
}
