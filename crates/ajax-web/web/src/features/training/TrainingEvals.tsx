import { useEffect, useState } from "react";
import { Button } from "@/shared/ui/button";
import Skeleton from "@/shared/ui/Skeleton";
import * as trainingApi from "./trainingApi";
import { evalGroupLabelParts, pathBasename } from "./trainingPageModel";

type EvalFilter = "all" | trainingApi.TrainingEvalKind;

function formatPercent(rate: number): string {
  return `${(rate * 100).toFixed(1)}%`;
}

function formatCreated(createdAt: string | null): string {
  if (!createdAt) return "—";
  const ms = Date.parse(createdAt);
  return Number.isFinite(ms) ? new Date(ms).toLocaleString() : createdAt;
}

function createdMs(evalRow: trainingApi.TrainingEval): number {
  const ms = evalRow.created_at ? Date.parse(evalRow.created_at) : NaN;
  return Number.isFinite(ms) ? ms : Number.NEGATIVE_INFINITY;
}

const EVAL_PAGE_SIZE = 20;

function championDeltaPoints(
  evalRow: trainingApi.TrainingEval,
  champion: trainingApi.TrainingChampion | null,
): number | null {
  if (!evalRow.suite || evalRow.pass_rate === null || !champion) return null;
  const championRate = champion.pass_rates[evalRow.suite];
  if (championRate === undefined) return null;
  return (evalRow.pass_rate - championRate) * 100;
}

function formatDelta(points: number): string {
  if (Math.abs(points) < 0.05) return "±0.0 pts";
  return `${points > 0 ? "+" : "-"}${Math.abs(points).toFixed(1)} pts`;
}

interface Props {
  evals: trainingApi.TrainingEvals | null;
  progress: trainingApi.TrainingEvalProgress[];
}

export default function TrainingEvals({ evals, progress }: Props) {
  const champion = evals?.champion ?? null;
  const [evalFilter, setEvalFilter] = useState<EvalFilter>("all");
  const [evalLimit, setEvalLimit] = useState(EVAL_PAGE_SIZE);
  const [expandedEval, setExpandedEval] = useState<string | null>(null);

  useEffect(() => {
    setEvalLimit(EVAL_PAGE_SIZE);
  }, [evalFilter]);

  const filteredEvals = (evals?.evals ?? [])
    .filter((evalRow) => evalFilter === "all" || evalRow.kind === evalFilter)
    .sort((a, b) => createdMs(b) - createdMs(a));
  const visibleEvals = filteredEvals.slice(0, evalLimit);
  const hasMoreEvals = filteredEvals.length > evalLimit;

  return (
    <section className="training-section" aria-labelledby="training-evals-heading">
      <h3 id="training-evals-heading">Evals</h3>
      {progress.map((entry) => {
        const { primary, qualifier } = evalGroupLabelParts(entry.group);
        return (
          <div
            key={JSON.stringify([entry.group, entry.checkpoint_label])}
            className="training-page-row"
            data-testid="training-eval-progress"
          >
            <div className="training-page-row-head">
              <span className="training-tag">Running</span>
              <span className="training-page-row-title">
                {primary}{qualifier ? <span className="training-muted"> {qualifier}</span> : null}
                <span className="training-muted"> · {entry.checkpoint_label}</span>
              </span>
            </div>
            <p>Suite {Math.min(entry.done.length + 1, entry.labels.length)} of {entry.labels.length}: {entry.current ?? "Waiting"}</p>
            <progress
              className="training-page-progress"
              aria-label={`Eval progress for ${entry.group} ${entry.checkpoint_label}`}
              value={entry.done.length}
              max={entry.labels.length}
            />
            <div className="training-page-badges">
              {entry.labels.map((label) => (
                <span key={label} className={`training-tag${entry.done.includes(label) ? " is-pass" : " is-muted"}`}>
                  {label}{entry.done.includes(label) ? " · done" : ""}
                </span>
              ))}
            </div>
            {entry.elapsed_s !== null ? <p className="training-muted">{Math.floor(entry.elapsed_s / 60)} min elapsed</p> : null}
          </div>
        );
      })}
      {evals === null ? (
        <Skeleton rows={3} testid="training-evals-loading" />
      ) : (
        <>
          <div className="training-page-champion" data-testid="training-champion">
            {champion ? (
              <>
                <p>
                  <strong>Champion</strong> {champion.run_id ?? "—"}
                  {champion.checkpoint ? (
                    <>
                      {" · "}
                      <span
                        className="training-page-champion-checkpoint"
                        title={champion.checkpoint}
                        data-testid="training-champion-checkpoint"
                      >
                        {pathBasename(champion.checkpoint)}
                      </span>
                    </>
                  ) : null}
                </p>
                {Object.keys(champion.pass_rates).length > 0 ? (
                  <p className="training-muted">
                    {Object.entries(champion.pass_rates)
                      .map(([suite, rate]) => `${suite} ${formatPercent(rate)}`)
                      .join(" · ")}
                  </p>
                ) : null}
              </>
            ) : (
              <p className="training-muted">No champion recorded.</p>
            )}
          </div>

          <div className="training-page-filter" role="group" aria-label="Eval filter">
            {(
              [
                ["all", "All"],
                ["checkpoint", "Checkpoints"],
                ["final", "Final"],
              ] as const
            ).map(([value, label]) => (
              <Button
                key={value}
                type="button"
                variant={evalFilter === value ? "default" : "secondary"}
                aria-pressed={evalFilter === value}
                onClick={() => setEvalFilter(value)}
              >
                {label}
              </Button>
            ))}
          </div>

          {filteredEvals.length === 0 ? (
            <p className="training-muted">No evals to show.</p>
          ) : (
            <>
            <ul className="training-page-list" data-testid="training-eval-list">
              {visibleEvals.map((evalRow) => {
                const expanded = expandedEval === evalRow.id;
                const deltaPoints = championDeltaPoints(evalRow, champion);
                const { primary: groupPrimary, qualifier: groupQualifier } = evalGroupLabelParts(
                  evalRow.group,
                );
                const gateText = evalRow.promotion_passed ? "Gate pass" : "Gate fail";
                const gateClass = evalRow.promotion_passed ? "is-pass" : "is-fail";
                return (
                  <li key={evalRow.id} className="training-page-row" data-testid="training-eval-row">
                    <button
                      type="button"
                      className="training-page-row-toggle"
                      aria-expanded={expanded}
                      aria-controls={`training-eval-details-${evalRow.id}`}
                      onClick={() => setExpandedEval(expanded ? null : evalRow.id)}
                    >
                      <span className="training-page-row-head">
                        <span className="training-tag">
                          {evalRow.kind === "final" ? "Final" : "Checkpoint"}
                        </span>
                        <span className="training-page-row-title">
                          {groupPrimary}
                          {groupQualifier ? (
                            <span className="training-muted"> {groupQualifier}</span>
                          ) : null}
                          {evalRow.kind === "checkpoint" && evalRow.checkpoint_label ? (
                            <span className="training-muted"> · {evalRow.checkpoint_label}</span>
                          ) : null}
                        </span>
                        <span className="training-muted">{formatCreated(evalRow.created_at)}</span>
                      </span>
                      <span className="training-page-metrics">
                        <span>{evalRow.suite ?? "ad-hoc"}</span>
                        <span data-testid="training-eval-rate">
                          {evalRow.pass_rate === null ? "—" : formatPercent(evalRow.pass_rate)}
                        </span>
                        <span>
                          {evalRow.passed ?? "—"}/{evalRow.total ?? "—"}
                        </span>
                        {evalRow.promotion_passed !== null ? (
                          <span className={gateClass}>{gateText}</span>
                        ) : null}
                        {deltaPoints !== null ? (
                          <span
                            className={deltaPoints >= 0 ? "is-pass" : "is-fail"}
                            data-testid="training-eval-delta"
                          >
                            {formatDelta(deltaPoints)}
                          </span>
                        ) : null}
                      </span>
                    </button>
                    {expanded ? (
                      <div id={`training-eval-details-${evalRow.id}`} className="training-page-row-details">
                        {evalRow.gate_label ? <p className="training-muted">{evalRow.gate_label}</p> : null}
                        <h4>Failures</h4>
                        {evalRow.failures.length > 0 ? (
                          <ul>
                            {evalRow.failures.map((failure, index) => (
                              <li key={`${index}-${failure}`}>{failure}</li>
                            ))}
                          </ul>
                        ) : (
                          <p className="training-muted">No failures recorded.</p>
                        )}
                        <h4>Categories</h4>
                        {Object.keys(evalRow.category_pass_rate).length > 0 ? (
                          <dl className="training-page-categories">
                            {Object.entries(evalRow.category_pass_rate).map(([category, rate]) => (
                              <div key={category}>
                                <dt>{category}</dt>
                                <dd>{formatPercent(rate)}</dd>
                              </div>
                            ))}
                          </dl>
                        ) : (
                          <p className="training-muted">No category breakdown.</p>
                        )}
                      </div>
                    ) : null}
                  </li>
                );
              })}
            </ul>
            {hasMoreEvals ? (
              <Button
                type="button"
                variant="secondary"
                className="training-page-show-more"
                data-testid="training-eval-show-more"
                onClick={() => setEvalLimit((limit) => limit + EVAL_PAGE_SIZE)}
              >
                Show more
              </Button>
            ) : null}
            </>
          )}
        </>
      )}
    </section>

  );
}
