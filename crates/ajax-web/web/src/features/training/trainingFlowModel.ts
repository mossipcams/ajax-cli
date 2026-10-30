import type {
  TrainingDataset, TrainingEval, TrainingEvalProgress, TrainingEvals,
  TrainingJobKind, TrainingStatus,
} from "./trainingApi";
import { formatBytes } from "./TrainingDatasets";
import { evalGroupLabelParts, formatEta, pathBasename, shortRunId } from "./trainingPageModel";

export type StageState = "done" | "active" | "next" | "pending" | "failed";

export interface FlowStage {
  label: string;
  job: TrainingJobKind;
  hardware: string;
  purpose: string;
  reads: string;
  writes: string;
  fact: string;
  state: StageState;
  stateText: string;
  actionVisible: boolean;
  actionDisabled: boolean;
  progress: { value: number; max: number } | null;
  hint?: string;
}

export interface FlowPoint {
  id: string;
  label: string;
  title: string;
  rate: number;
  x: number;
  y: number;
  best: boolean;
  champion: boolean;
}

export interface FlowChart {
  points: FlowPoint[];
  first: string;
  last: string;
  summary: string;
  reference: { y: number; label: string } | null;
}

const stateText: Record<StageState, string> = {
  done: "Done", active: "Running", next: "Next", pending: "Pending", failed: "Failed",
};

function percent(rate: number): string {
  return `${(rate * 100).toFixed(1)}%`;
}

function latest(rows: TrainingEval[]): TrainingEval | undefined {
  return [...rows].sort((a, b) => (b.created_at ?? "").localeCompare(a.created_at ?? ""))[0];
}

function result(row: TrainingEval | undefined, fallback: string): string {
  return row?.pass_rate != null
    ? `${row.suite ?? "Suite"} · ${percent(row.pass_rate)} pass · ${row.checkpoint_label ?? "checkpoint unknown"}`
    : fallback;
}

function liveEval(entry: TrainingEvalProgress) {
  return {
    id: `${entry.group}/${entry.checkpoint_label}`,
    label: `${entry.group} · ${entry.checkpoint_label}`,
    fact: `${entry.done.length}/${entry.labels.length} suites complete${entry.current ? ` · ${entry.current}` : ""}`,
    value: entry.done.length,
    max: entry.labels.length,
  };
}

function chart(rows: TrainingEval[], sweep: boolean, champion: TrainingEvals["champion"]): FlowChart {
  const steps = rows.map((row) => Number(row.checkpoint_label?.slice("checkpoint-".length)));
  const min = Math.min(...steps);
  const max = Math.max(...steps);
  const bestRate = Math.max(...rows.map((row) => row.pass_rate ?? 0));
  const checkpoint = champion?.checkpoint ? pathBasename(champion.checkpoint) : null;
  const points = rows.map((row, index): FlowPoint => {
    const rate = row.pass_rate ?? 0;
    return {
      id: row.id,
      label: `${sweep ? "" : `${evalGroupLabelParts(row.group).primary} · `}${row.checkpoint_label ?? "checkpoint unknown"} · ${percent(rate)}${sweep && rate === bestRate ? " · Best" : ""}${sweep && champion?.run_id && row.group.startsWith(`${champion.run_id}-`) && row.checkpoint_label === checkpoint ? " · Champion" : ""}`,
      title: `${row.group} · ${row.checkpoint_label ?? "checkpoint unknown"} · ${percent(rate)}`,
      rate,
      x: sweep ? (max === min ? 50 : 12 + (steps[index] - min) / (max - min) * 84)
        : (rows.length === 1 ? 50 : 12 + index / (rows.length - 1) * 84),
      y: 88 - Math.max(0, Math.min(1, rate)) * 76,
      best: sweep && rate === bestRate,
      champion: Boolean(sweep && champion?.run_id && row.group.startsWith(`${champion.run_id}-`) && row.checkpoint_label === checkpoint),
    };
  });
  const referenceRate = champion?.pass_rates.promotion;
  return {
    points,
    first: sweep ? `Step ${min}` : "Older",
    last: sweep ? `Step ${max}` : "Latest",
    summary: sweep ? points.find((point) => point.best)?.label ?? "" : "Latest promotion-suite experiments · time order",
    reference: !sweep && referenceRate != null
      ? { y: 88 - Math.max(0, Math.min(1, referenceRate)) * 76, label: `Champion · ${percent(referenceRate)}` }
      : null,
  };
}

export function trainingFlowModel(
  status: TrainingStatus,
  evals: TrainingEvals | null,
  datasets: TrainingDataset[] | null,
  jobsDisabled = false,
  championRunId: string | null = evals?.champion?.run_id ?? null,
) {
  const pipeline = status.pipeline;
  const selectedId = pipeline?.run_id ?? null;
  const promotedId = pipeline?.promoted_run_id ?? null;
  const selected = datasets?.find((row) => row.run_id === selectedId);
  const promoted = datasets?.find((row) => row.promoted && row.run_id === promotedId);
  const trainRecord = promoted ?? (promotedId === selectedId ? pipeline : null);
  const trained = trainRecord?.trained === true;
  const finalPassed = trainRecord?.final_eval_passed === true;
  const rows = evals?.evals ?? [];
  const matches = (group: string, id: string | null, suffix = "") => id !== null && group.startsWith(`${id}-${suffix}`);
  const canaryRows = rows.filter((row) => matches(row.group, selectedId, "canary"));
  const finalRows = rows.filter((row) => matches(row.group, promotedId, "final-eval") && row.suite === "promotion");
  const sweepRows = rows.filter((row) => row.kind === "checkpoint" && row.suite === "promotion"
    && matches(row.group, promotedId, "select-checkpoint") && row.pass_rate !== null
    && /^checkpoint-\d+$/.test(row.checkpoint_label ?? ""));
  const newestPerStep = new Map<string, TrainingEval>();
  for (const row of [...sweepRows].sort((a, b) => (a.created_at ?? "").localeCompare(b.created_at ?? ""))) {
    newestPerStep.set(row.checkpoint_label ?? "", row);
  }
  const sweep = [...newestPerStep.values()].sort((a, b) =>
    Number(a.checkpoint_label?.slice(11)) - Number(b.checkpoint_label?.slice(11)));
  const experiments = datasets === null ? [] : rows.filter((row) => row.suite === "promotion"
    && row.pass_rate !== null && !datasets.some((dataset) => row.group.startsWith(dataset.run_id)))
    .sort((a, b) => (a.created_at ?? "").localeCompare(b.created_at ?? "")).slice(-20);
  const liveTraining = status.eval_progress.filter((entry) => matches(entry.group, promotedId));
  const liveExperiments = status.eval_progress.filter((entry) => !matches(entry.group, promotedId));
  const running = (kind: string) => status.run?.running === true && status.run.kind === kind;
  const trainRunning = running("lfm-train") && status.device === "gpu" && status.state.startsWith("train:");
  const gen = status.generation?.running ? status.generation : null;
  const progress = trainRunning ? status.run?.progress : null;
  const trainFact = progress
    ? `Step ${progress.step}${progress.total ? ` of ${progress.total}` : ""}${progress.loss !== null ? ` · loss ${progress.loss.toFixed(4)}` : ""}${progress.eta_s !== null ? ` · ETA ${formatEta(progress.eta_s)}` : ""}`
    : trained ? "Full training complete" : trainRecord ? "Awaiting full training" : "Training record unavailable";
  const champion = evals?.champion ?? null;
  const championCheckpoint = champion?.checkpoint ? pathBasename(champion.checkpoint) : "Checkpoint unknown";
  const selectedPath = selectedId ? `candidates/${selectedId}.jsonl` : "Selected candidate rows";
  const fullPath = promotedId ? `${promotedId}-full/` : "<promoted run>-full/";
  let nextAssigned = false;
  function stage(
    job: TrainingJobKind, label: string, hardware: string, purpose: string, reads: string,
    writes: string, fact: string, done: boolean, active: boolean, failed = false,
    progress: FlowStage["progress"] = null, eligible = true,
  ): FlowStage {
    const state: StageState = active ? "active" : done ? "done" : failed ? "failed"
      : !nextAssigned && eligible ? "next" : "pending";
    if (state !== "done") nextAssigned = true;
    const actionVisible = state !== "active"
      && (job === "lfm-train" || state === "next" || state === "failed" || job === "select-checkpoint");
    const selectedMismatch = (job === "lfm-eval" || job === "promote-model") && selectedId !== promotedId;
    return {
      job, label, hardware, purpose, reads, writes, fact, state, stateText: stateText[state],
      progress, actionVisible,
      actionDisabled: jobsDisabled || selectedMismatch || (job === "lfm-train" && (promotedId === null || pipeline?.promoted_record_present !== true || trained)),
      hint: selectedMismatch
        ? `Runs on the selected dataset (${selectedId ? shortRunId(selectedId) : "none"}). ${promotedId ? `Select ${shortRunId(promotedId)} in Datasets to run it here.` : "Promote a dataset first."}`
        : undefined,
    };
  }
  const trackA = pipeline ? [
    stage("generate", "Generate", "GPU", "The serving LLM writes candidate training rows.",
      "Generation prompts → serving LLM", "candidates/<new run_id>.jsonl + manifest; current.json selects the new dataset",
      gen ? `Rows ${gen.rows ?? 0}${gen.target !== null ? ` of ${gen.target}` : ""}${gen.phase ? ` · ${gen.phase}` : ""} · live generation, not tied to a run`
        : `${shortRunId(pipeline.run_id)} · ${formatBytes(selected?.size_bytes ?? null)}`,
      true, gen !== null || running("generate"), false,
      gen?.target ? { value: gen.rows ?? 0, max: gen.target } : null),
    stage("validate", "Validate", "CPU", "Check the rows and lock their file fingerprint.",
      selectedPath, "Validated run record + locked sha256",
      pipeline.validated ? "sha256 locked" : "Preflight checks pending", pipeline.validated, running("validate")),
    stage("canary", "Canary", "GPU → CPU", "Try a short train, then smoke-test its checkpoint.",
      selectedPath, "Canary checkpoint + smoke-suite eval",
      result(latest(canaryRows.filter((row) => row.suite === "smoke")), pipeline.canary_passed ? "Canary passed" : "No smoke result yet"),
      pipeline.canary_passed, running("canary") || status.eval_progress.some((entry) => matches(entry.group, selectedId, "canary")),
      !pipeline.canary_passed && canaryRows.length > 0),
    stage("promote-dataset", "Promote dataset", "—", "Point full training at this dataset.",
      "Validated, canary-checked dataset", "promoted_dataset.json",
      pipeline.dataset_promoted ? "Dataset promotion recorded" : "Pointer not updated for this dataset",
      pipeline.dataset_promoted, running("promote-dataset")),
  ] : [];
  nextAssigned = false;
  const train = stage("lfm-train", "Train", "GPU", "Unsloth fine-tunes on the promoted dataset.",
    "promoted_dataset.json → candidate rows", `${fullPath}checkpoint-N … final`, trainFact,
    trained, trainRunning, false, progress?.total ? { value: progress.step, max: progress.total } : null, promotedId !== null);
  train.hint = promotedId === null ? "Promote a dataset first" : `Trains ${shortRunId(promotedId)}`;
  const trackB = pipeline ? [train] : [];
  if (pipeline && status.checkpoint_eval && pipeline.trained) {
    const live = liveTraining.find((entry) => matches(entry.group, promotedId, "select-checkpoint"));
    trackB.push(stage("select-checkpoint", "Evaluate checkpoints", "CPU", "Compare checkpoints on the promotion suite.",
      `${fullPath}checkpoint-N`, "Per-checkpoint suite scores", live ? liveEval(live).fact : `${sweep.length} checkpoints scored`,
      sweep.length > 0, running("select-checkpoint") || Boolean(live), false,
      live ? { value: live.done.length, max: live.labels.length } : null));
  }
  if (pipeline) {
    const live = liveTraining.find((entry) => matches(entry.group, promotedId, "final-eval"));
    trackB.push(stage("lfm-eval", "Eval", "CPU", "Score the trained run's checkpoint on the promotion suite (the promotion gate).",
      "current.json → trained run's checkpoint", "Promotion-suite result", result(latest(finalRows), finalPassed ? "Final eval passed" : "No final result yet"),
      finalPassed, running("lfm-eval") || Boolean(live), !finalPassed && finalRows.length > 0,
      live ? { value: live.done.length, max: live.labels.length } : null));
    trackB.push(stage("promote-model", "Promote model", "—", "Record the champion checkpoint and suite scores.",
      "current.json → evaluated checkpoint + suite results", "promoted_model.json",
      champion ? `Champion ${championCheckpoint}` : "No champion recorded",
      promotedId !== null && championRunId === promotedId, running("promote-model")));
  }
  const profile = status.active_profile ? status.profile_details?.[status.active_profile] : null;
  return {
    trackA, trackB,
    selected: { id: selectedId, token: selectedId ? shortRunId(selectedId) : "None selected", size: formatBytes(selected?.size_bytes ?? null) },
    promoted: { id: promotedId, token: promotedId ? shortRunId(promotedId) : "None promoted", size: formatBytes(promoted?.size_bytes ?? null) },
    handoff: {
      note: promotedId && promotedId !== selectedId ? `Train uses promoted dataset ${promotedId}, not the selected one` : null,
      missing: promotedId && !pipeline?.promoted_record_present ? `Promoted dataset ${promotedId} is missing from this checkout — Train will fail` : null,
      destination: promotedId ? `${shortRunId(promotedId)} → Train & evaluate` : "Promote a dataset to connect Track B",
    },
    now: {
      gpu: status.state === "serve" ? `Serving ${profile?.label ?? profile?.model ?? status.active_profile ?? "active profile"} (llama.cpp)`
        : status.state.startsWith("train:") ? "Training" : "Idle",
      cpu: status.eval_progress.length ? `${status.eval_progress.length} running eval${status.eval_progress.length === 1 ? "" : "s"}` : "Idle",
      model: status.state === "serve" ? profile?.model ?? null : null,
    },
    sweep: sweep.length ? chart(sweep, true, champion) : null,
    experiments: experiments.length ? chart(experiments, false, champion) : null,
    experimentsUnavailable: datasets === null,
    liveTraining: liveTraining.map(liveEval),
    liveExperiments: liveExperiments.map(liveEval),
    champion: champion ? {
      run: champion.run_id ? shortRunId(champion.run_id) : "Run unknown",
      fullRun: champion.run_id, checkpoint: championCheckpoint,
      rates: Object.entries(champion.pass_rates).map(([suite, rate]) => ({ suite, rate, text: percent(rate) })),
    } : null,
  };
}

export type TrainingFlow = ReturnType<typeof trainingFlowModel>;
