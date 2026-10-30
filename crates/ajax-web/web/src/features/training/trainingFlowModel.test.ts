import { describe, expect, it } from "vitest";
import type { TrainingDataset, TrainingEval, TrainingEvals, TrainingStatus } from "./trainingApi";
import { trainingFlowModel } from "./trainingFlowModel";

const selectedId = "20261018T221221Z-6acd67f1";
const promotedId = "20261017T203638Z-962e98a1";

function dataset(run_id: string, promoted: boolean): TrainingDataset {
  return { run_id, promoted, current: !promoted, size_bytes: 2048, validated: true,
    canary_passed: promoted, dataset_promoted: promoted, trained: promoted, final_eval_passed: promoted };
}

function evaluation(group: string, checkpoint_label: string, pass_rate: number, overrides: Partial<TrainingEval> = {}): TrainingEval {
  return { id: `${group}/${checkpoint_label}`, group, checkpoint_label, pass_rate, kind: "checkpoint", suite: "promotion",
    gate_label: null, created_at: "2026-10-20T00:00:00Z", total: 120, passed: 95,
    promotion_passed: null, failures: [], category_pass_rate: {}, ...overrides };
}

function fixture() {
  const datasets = [dataset(selectedId, false), dataset(promotedId, true)];
  const status: TrainingStatus = {
    ok: true, state: "serve", runtime_up: true, active_profile: "writer", profiles: ["writer"],
    profile_details: { writer: { label: "Dataset writer", model: "writer.gguf" } },
    run: null, generation: { running: false, rows: 900, target: 1000, phase: "done" },
    device: null, checkpoint_eval: true, eval_progress: [],
    pipeline: { ...datasets[0], promoted_run_id: promotedId, promoted_record_present: true },
  };
  const evals: TrainingEvals = {
    champion: { run_id: promotedId, checkpoint: `/models/${promotedId}-full/checkpoint-3000`, pass_rates: { promotion: 0.7917, context: 1 } },
    evals: [
      ...[3000, 3200, 3400, 3600, 3800, 4000].map((step) => evaluation(`${promotedId}-select-checkpoint`, `checkpoint-${step}`,
        step === 3000 ? 0.7917 : step === 3600 || step === 3800 ? 0.7667 : 0.7833)),
      evaluation(`${promotedId}-canary`, "final", 0.5, { suite: "smoke", kind: "final" }),
      evaluation(`${promotedId}-final-eval`, "final", 0.75, { kind: "final" }),
      evaluation("v13-ckpt1000-promotion", "checkpoint-1000", 0.7),
      evaluation("frozen-ckpt400-promotion", "checkpoint-400", 0.6),
      evaluation("v12-full-ckpt1000-eval", "checkpoint-1000", 0.65),
    ],
  };
  return { status, datasets, evals };
}

function stage(flow: ReturnType<typeof trainingFlowModel>, label: string) {
  return [...flow.trackA, ...flow.trackB].find((entry) => entry.label === label);
}

describe("trainingFlowModel", () => {
  it.each([false, true])("guards selected-dataset actions when final eval passed is %s", (finalPassed) => {
    const { status, evals, datasets } = fixture();
    datasets[1].final_eval_passed = finalPassed;
    evals.champion = null;
    evals.evals = [];
    const label = finalPassed ? "Promote model" : "Eval";
    const mismatched = trainingFlowModel(status, evals, datasets);
    expect(stage(mismatched, label)).toMatchObject({ state: "next", actionVisible: true, actionDisabled: true });
    for (const action of ["Eval", "Promote model"]) {
      expect(stage(mismatched, action)?.hint).toBe("Runs on the selected dataset (6acd67f1). Select 962e98a1 in Datasets to run it here.");
      expect(stage(mismatched, action)?.actionDisabled).toBe(true);
    }
    status.pipeline = { ...status.pipeline!, run_id: promotedId, trained: true };
    status.checkpoint_eval = false;
    const aligned = trainingFlowModel(status, evals, datasets);
    expect(stage(aligned, label)).toMatchObject({ state: "next", actionVisible: true, actionDisabled: false });
    expect(stage(aligned, label)?.hint).toBeUndefined();
    expect(stage(trainingFlowModel(status, evals, datasets, true), label)?.actionDisabled).toBe(true);
  });

  it("keeps the selected dataset separate from the trained promoted dataset", () => {
    const { status, evals, datasets } = fixture();
    const flow = trainingFlowModel(status, evals, datasets);
    expect(flow.selected.id).toBe(selectedId);
    expect(flow.promoted.id).toBe(promotedId);
    expect(stage(flow, "Validate")?.state).toBe("done");
    expect(stage(flow, "Canary")?.state).toBe("next");
    expect(stage(flow, "Train")?.state).toBe("done");
    expect(stage(flow, "Train")?.actionDisabled).toBe(true);
    expect(stage(flow, "Eval")?.state).toBe("done");
    expect(stage(flow, "Eval")?.fact).toContain("75.0%");
    expect(flow.handoff.note).toBe(`Train uses promoted dataset ${promotedId}, not the selected one`);
    expect(stage(flow, "Generate")?.fact).not.toContain("900");
    expect(stage(flow, "Evaluate checkpoints")).toBeUndefined();
  });

  it("builds the checkpoint sweep from promotion scores and marks its best and champion", () => {
    const { status, evals, datasets } = fixture();
    evals.evals.push(evaluation(`${promotedId}-select-checkpoint`, "final", 1));
    evals.evals.push(evaluation(`${promotedId}-select-checkpoint`, "checkpoint-3000", 1, { suite: "context" }));
    const flow = trainingFlowModel(status, evals, datasets);
    expect(flow.sweep?.points).toHaveLength(6);
    expect(flow.sweep?.points[0]).toMatchObject({ rate: 0.7917, best: true, champion: true, x: 12 });
    expect(flow.sweep?.points[0].label).toBe("checkpoint-3000 · 79.2% · Best · Champion");
    expect(flow.sweep?.points[0].title).toContain(`${promotedId}-select-checkpoint`);
    expect(flow.sweep?.summary).toBe("checkpoint-3000 · 79.2% · Best · Champion");
    expect(flow.sweep?.points[5]).toMatchObject({ rate: 0.7833, best: false, champion: false, x: 96 });
    expect(flow.champion?.checkpoint).toBe("checkpoint-3000");
    expect(flow.champion?.rates).toEqual([
      { suite: "promotion", rate: 0.7917, text: "79.2%" }, { suite: "context", rate: 1, text: "100.0%" },
    ]);
  });

  it("groups only unknown dataset runs as experiments and keeps the latest 20 in time order", () => {
    const { status, evals, datasets } = fixture();
    const first = trainingFlowModel(status, evals, datasets);
    expect(first.experiments?.points).toHaveLength(3);
    expect(first.experiments?.points[0].label).toBe("v13-ckpt1000-promotion · checkpoint-1000 · 70.0%");
    expect(first.experiments?.reference?.label).toBe("Champion · 79.2%");
    evals.evals.push(...Array.from({ length: 25 }, (_, index) => evaluation(`experiment-${index}`, "final", index / 25,
      { created_at: `2026-10-${String(index + 1).padStart(2, "0")}T00:00:00Z` })));
    const flow = trainingFlowModel(status, evals, datasets);
    expect(flow.experiments?.points).toHaveLength(20);
    expect(flow.experiments?.points.at(-1)?.label).toContain("experiment-24");
    expect(flow.experiments?.points.some((point) => point.label.includes("experiment-0 ·"))).toBe(false);
    expect(trainingFlowModel(status, evals, null).experiments).toBeNull();
  });

  it("routes live evals by promoted run and shows actual hardware activity", () => {
    const { status, evals, datasets } = fixture();
    status.eval_progress = [promotedId + "-select-checkpoint", "v13-promotion"].map((group) => ({
      group, checkpoint_label: "checkpoint-3200", labels: ["promotion", "context"], done: ["promotion"], current: "context", elapsed_s: 20,
    }));
    const flow = trainingFlowModel(status, evals, datasets);
    expect(flow.liveTraining).toHaveLength(1);
    expect(flow.liveExperiments).toHaveLength(1);
    expect(flow.liveTraining[0]).toMatchObject({ value: 1, max: 2, fact: "1/2 suites complete · context" });
    expect(flow.now.gpu).toBe("Serving Dataset writer (llama.cpp)");
    expect(flow.now.model).toBe("writer.gguf");
    expect(flow.now.cpu).toBe("2 running evals");
  });

  it("does not borrow selected training flags when a different promoted record is unavailable", () => {
    const { status, evals, datasets } = fixture();
    status.pipeline = { ...status.pipeline!, trained: true, final_eval_passed: true, promoted_record_present: false };
    const flow = trainingFlowModel(status, evals, [datasets[0]]);
    expect(stage(flow, "Train")?.state).toBe("next");
    expect(stage(flow, "Train")?.fact).toBe("Training record unavailable");
    expect(stage(flow, "Train")?.actionDisabled).toBe(true);
    expect(flow.handoff.missing).toContain("is missing from this checkout");
    expect(stage(flow, "Eval")?.state).not.toBe("done");
  });

  it("enables Train from the promoted record, regardless of the selected training flag", () => {
    const { status, evals, datasets } = fixture();
    status.pipeline = { ...status.pipeline!, trained: true };
    datasets[1].trained = false;
    expect(stage(trainingFlowModel(status, evals, datasets), "Train")?.actionDisabled).toBe(false);
    expect(stage(trainingFlowModel(status, evals, datasets, true), "Train")?.actionDisabled).toBe(true);
    status.pipeline.promoted_run_id = null;
    const flow = trainingFlowModel(status, evals, datasets);
    expect(stage(flow, "Train")?.actionDisabled).toBe(true);
    expect(stage(flow, "Train")?.hint).toBe("Promote a dataset first");
  });

  it("shows running generation without treating its rows as the selected dataset size", () => {
    const { status, evals, datasets } = fixture();
    status.generation = { running: true, rows: 40, target: 100, phase: "writing" };
    const flow = trainingFlowModel(status, evals, datasets);
    expect(stage(flow, "Generate")).toMatchObject({ state: "active", progress: { value: 40, max: 100 } });
    expect(stage(flow, "Generate")?.fact).toContain("not tied to a run");
    expect(flow.selected.size).toBe("2.0 KB");
  });

  it("shows GPU training progress only with live training evidence", () => {
    const { status, evals, datasets } = fixture();
    status.state = "train:lfm";
    status.device = "gpu";
    status.run = { kind: "lfm-train", running: true, started: "", external: false, log_tail: [],
      progress: { step: 12, total: 100, loss: 0.5, eta_s: 120 } };
    const flow = trainingFlowModel(status, evals, datasets);
    expect(stage(flow, "Train")).toMatchObject({ state: "active", progress: { value: 12, max: 100 } });
    expect(stage(flow, "Train")?.fact).toBe("Step 12 of 100 · loss 0.5000 · ETA ~2 min");
    expect(flow.now.gpu).toBe("Training");
    expect(flow.now.model).toBeNull();
    status.run.running = false;
    expect(stage(trainingFlowModel(status, evals, datasets), "Train")?.progress).toBeNull();
  });

  it("shows failed canary evidence without confusing canary results with final eval", () => {
    const { status, evals, datasets } = fixture();
    evals.evals.push(evaluation(`${selectedId}-canary-retry`, "final", 0.25, { kind: "final", suite: "smoke" }));
    const flow = trainingFlowModel(status, evals, datasets);
    expect(stage(flow, "Canary")?.state).toBe("failed");
    expect(stage(flow, "Canary")?.fact).toContain("25.0%");
    expect(stage(flow, "Eval")?.fact).toContain("75.0%");
  });

  it("hides the start action on a running lfm-train stage", () => {
    const { status, evals, datasets } = fixture();
    status.state = "train:lfm";
    status.device = "gpu";
    status.run = { kind: "lfm-train", running: true, started: "", external: false, log_tail: [], progress: null };
    const flow = trainingFlowModel(status, evals, datasets);
    expect(stage(flow, "Train")).toMatchObject({ state: "active", actionVisible: false });
  });

  it("hides empty plots and handles missing pipeline and champion", () => {
    const { status } = fixture();
    status.pipeline = null;
    status.state = "idle";
    const flow = trainingFlowModel(status, null, []);
    expect(flow.trackA).toEqual([]);
    expect(flow.trackB).toEqual([]);
    expect(flow.sweep).toBeNull();
    expect(flow.experiments).toBeNull();
    expect(flow.champion).toBeNull();
    expect(flow.now).toMatchObject({ gpu: "Idle", cpu: "Idle" });
  });
});
