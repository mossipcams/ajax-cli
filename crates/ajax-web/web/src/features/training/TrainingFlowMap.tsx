import { useState, type ReactNode } from "react";
import { Button } from "@/shared/ui/button";
import type { PageAction } from "./trainingPageModel";
import type { FlowChart, FlowStage, TrainingFlow } from "./trainingFlowModel";

function Stage({ stage, onAction, children }: { stage: FlowStage; onAction: (action: PageAction) => void; children?: ReactNode }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <li className={`training-map-stage is-${stage.state}`} data-testid="training-pipeline-step" data-state={stage.state}>
      <span className="training-map-node" aria-hidden="true" />
      <div className="training-map-tile">
        <details open={expanded}>
          <summary className="training-map-summary" aria-expanded={expanded}
            onClick={(event) => { event.preventDefault(); setExpanded(!expanded); }}>
            <span className="training-map-stage-head">
              <strong>{stage.label}</strong>
              <span className="training-map-hardware">{stage.hardware}</span>
              <span className="training-map-state">{stage.stateText}</span>
            </span>
            <span className="training-map-purpose">{stage.purpose}</span>
            <span className="training-map-fact">{stage.fact}</span>
            <span className="training-map-disclosure">How it works</span>
          </summary>
          {expanded ? <dl className="training-map-explainer">
            <dt>What it does</dt><dd>{stage.purpose}</dd>
            <dt>Reads</dt><dd>{stage.reads}</dd>
            <dt>Writes</dt><dd>{stage.writes}</dd>
            <dt>Latest result</dt><dd>{stage.fact}</dd>
          </dl> : null}
        </details>
        {stage.state === "active" ? (
          <progress className="training-page-progress" aria-label={`${stage.label} stage progress`}
            {...(stage.progress ? { value: stage.progress.value, max: stage.progress.max } : {})} />
        ) : null}
        {stage.hint ? <p className="training-muted" data-testid={stage.job === "lfm-train" ? "training-train-hint" : undefined}>{stage.hint}</p> : null}
        {stage.actionVisible ? (
          <Button type="button" disabled={stage.actionDisabled} onClick={() => onAction({ kind: "job", job: stage.job })}>
            Start {stage.label.toLowerCase()}
          </Button>
        ) : null}
      </div>
      {children}
    </li>
  );
}

function ScorePlot({ chart, title, connected = false }: { chart: FlowChart; title: string; connected?: boolean }) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = chart.points.find((point) => point.id === selectedId);
  return (
    <figure className="training-map-chart" aria-label={title}>
      <figcaption>{title}<span className="training-muted">Promotion pass rate</span></figcaption>
      <div className="training-map-plot">
        <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
          <path d="M12 12H96 M12 50H96 M12 88H96" className="training-map-grid" />
          {chart.reference ? <path d={`M12 ${chart.reference.y}H96`} className="training-map-reference" /> : null}
          {connected ? <polyline points={chart.points.map((point) => `${point.x},${point.y}`).join(" ")} className="training-map-line" /> : null}
        </svg>
        <span className="training-map-axis is-top">100%</span>
        <span className="training-map-axis is-middle">50%</span>
        <span className="training-map-axis is-bottom">0%</span>
        {chart.points.map((point) => (
          <button key={point.id} type="button"
            className={`training-map-point${point.best ? " is-best" : ""}${point.champion ? " is-champion" : ""}`}
            style={{ left: `${point.x}%`, top: `${point.y}%` }}
            aria-label={point.label} title={point.title} aria-pressed={selectedId === point.id}
            onFocus={() => setSelectedId(point.id)} onClick={() => setSelectedId(point.id)}>
            <svg viewBox="0 0 18 18" role="img">
              <title>{point.title}</title>
              {point.champion ? <text x="9" y="9" textAnchor="middle" dominantBaseline="central" fill="currentColor">★</text> : null}
            </svg>
          </button>
        ))}
      </div>
      <div className="training-map-axis-labels"><span>{chart.first}</span><span>{chart.last}</span></div>
      {chart.reference ? <p className="training-map-reference-label">{chart.reference.label} — reference</p> : null}
      <p className="training-map-chart-caption" aria-live="polite">{selected?.label ?? chart.summary}</p>
      <p className="training-muted">Tap a point for its group and checkpoint.{connected ? " Filled = best · ★ = champion." : ""}</p>
    </figure>
  );
}

function LiveEvals({ items }: { items: TrainingFlow["liveTraining"] }) {
  return items.length ? (
    <ul className="training-map-live" aria-label="Running evaluations">
      {items.map((item) => (
        <li key={item.id}>
          <strong>CPU · Evaluating</strong>
          <span>{item.label}</span>
          <span className="training-muted">{item.fact}</span>
          <progress className="training-page-progress" aria-label={`Eval progress: ${item.label}`} value={item.value} max={item.max} />
        </li>
      ))}
    </ul>
  ) : null;
}

export default function TrainingFlowMap({ flow, onAction }: { flow: TrainingFlow; onAction: (action: PageAction) => void }) {
  return (
    <div className="training-map">
      <div className="training-map-now" aria-label="Hardware now">
        <strong className="training-map-eyebrow">Now</strong>
        <div><span className="training-map-hardware">GPU</span><div>{flow.now.gpu}{flow.now.model ? <p className="training-muted">{flow.now.model}</p> : null}</div></div>
        <div><span className="training-map-hardware">CPU</span><span>{flow.now.cpu}</span></div>
      </div>
      {flow.trackA.length ? (
        <>
          <section className="training-map-lane is-dataset" aria-labelledby="training-map-build">
            <header>
              <span className="training-map-eyebrow">Track A · selected dataset</span>
              <h4 id="training-map-build">Build a dataset</h4>
              <p><code title={flow.selected.id ?? undefined}>{flow.selected.token}</code><span>{flow.selected.size}</span></p>
            </header>
            <ol className="training-map-rail">{flow.trackA.map((stage) => <Stage key={stage.job} stage={stage} onAction={onAction} />)}</ol>
          </section>
          <div className="training-map-handoff" aria-label="Dataset handoff">
            <span className="training-map-eyebrow">Handoff · pointer file</span>
            <strong>promoted_dataset.json</strong>
            <div className="training-map-tokens">
              <span>Selected <code title={flow.selected.id ?? undefined}>{flow.selected.token}</code></span>
              <span>Promoted <code title={flow.promoted.id ?? undefined}>{flow.promoted.token}</code></span>
            </div>
            {flow.handoff.note ? <p role="note" data-testid="training-train-promoted-note">{flow.handoff.note}</p> : null}
            {flow.handoff.missing ? <p role="alert" className="training-banner" data-testid="training-train-missing-promoted">{flow.handoff.missing}</p> : null}
            <p className="training-map-destination">↓ {flow.handoff.destination}</p>
          </div>
          <section className="training-map-lane is-training" aria-labelledby="training-map-train">
            <header>
              <span className="training-map-eyebrow">Track B · promoted dataset</span>
              <h4 id="training-map-train">Train &amp; evaluate</h4>
              <p><code title={flow.promoted.id ?? undefined}>{flow.promoted.token}</code><span>{flow.promoted.size}</span></p>
            </header>
            <ol className="training-map-rail">
              {flow.trackB.map((stage) => (
                <Stage key={stage.job} stage={stage} onAction={onAction}>
                  {stage.job === "lfm-train" && flow.sweep ? <ScorePlot chart={flow.sweep} title="Checkpoint sweep" connected /> : null}
                </Stage>
              ))}
            </ol>
            <LiveEvals items={flow.liveTraining} />
          </section>
        </>
      ) : <p className="training-muted">No dataset pipeline for the current run yet.</p>}
      <section className="training-map-champion" aria-label="Champion">
        <div className="training-map-champion-heading"><svg viewBox="0 0 32 32" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="M9 4h14v9a7 7 0 0 1-14 0V4ZM9 7H4v4a6 6 0 0 0 6 6M23 7h5v4a6 6 0 0 1-6 6M16 20v7M10 28h12" /></svg><div><span className="training-map-eyebrow">promoted_model.json</span><h4>Champion</h4></div></div>
        {flow.champion ? (
          <>
            <strong>{flow.champion.checkpoint}</strong>
            <code title={flow.champion.fullRun ?? undefined}>{flow.champion.run}</code>
            {flow.champion.rates.map(({ suite, rate, text }) => (
              <div className="training-map-score" key={suite}>
                <span>{suite}</span><strong>{text}</strong>
                <meter min={0} max={1} value={rate} aria-label={`Champion ${suite} pass rate`}>{text}</meter>
              </div>
            ))}
          </>
        ) : <p className="training-muted">No champion recorded.</p>}
      </section>
      <section className="training-map-experiments" aria-labelledby="training-map-experiments">
        <span className="training-map-eyebrow">Separate runs</span>
        <h4 id="training-map-experiments">Experiments</h4>
        <p className="training-muted">Ad-hoc evaluations outside known dataset runs.</p>
        {flow.experiments ? <ScorePlot chart={flow.experiments} title="Experiment scores" />
          : <p className="training-muted">{flow.experimentsUnavailable ? "Load the dataset catalog to identify experiments." : "No promotion-suite experiments yet."}</p>}
        <LiveEvals items={flow.liveExperiments} />
      </section>
    </div>
  );
}
