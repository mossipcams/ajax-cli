import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@/test/testing-library-shim";
import TrainingPipeline from "./TrainingPipeline";
import type { TrainingDataset, TrainingEvals, TrainingStatus } from "./trainingApi";

const selectedId = "20261018T221221Z-6acd67f1";
const promotedId = "20261017T203638Z-962e98a1";
const datasets: TrainingDataset[] = [
  { run_id: selectedId, current: true, promoted: false, size_bytes: 2048, validated: true,
    canary_passed: false, dataset_promoted: false, trained: false, final_eval_passed: false },
  { run_id: promotedId, current: false, promoted: true, size_bytes: 4096, validated: true,
    canary_passed: true, dataset_promoted: true, trained: true, final_eval_passed: true },
];
function status(overrides: Partial<TrainingStatus> = {}): TrainingStatus {
  return { ok: true, state: "serve", runtime_up: true, active_profile: "writer", profiles: ["writer"],
    profile_details: { writer: { label: "Writer", model: "writer.gguf" } }, run: null, generation: null,
    device: null, checkpoint_eval: true, eval_progress: [],
    pipeline: { ...datasets[0], promoted_run_id: promotedId, promoted_record_present: true }, ...overrides };
}
function mount(overrides: Partial<TrainingStatus> = {}, evals: TrainingEvals | null = null, jobsDisabled = false) {
  const onAction = vi.fn();
  render(<TrainingPipeline status={status(overrides)} datasets={datasets} evals={evals}
    championRunId={evals?.champion?.run_id ?? null} jobsDisabled={jobsDisabled} onAction={onAction} />);
  return onAction;
}
function stage(label: string) {
  const tile = screen.getAllByTestId("training-pipeline-step").find((entry) => within(entry).queryByText(label, { exact: true }));
  if (!tile) throw new Error(`Missing stage ${label}`);
  return tile;
}

describe("Training map", () => {
  it("names both dataset lanes and the exact pointer handoff without a stage counter", () => {
    mount();
    expect(screen.getByRole("heading", { name: "Pipeline" })).toBeVisible();
    expect(screen.getByRole("region", { name: "Build a dataset" })).toHaveTextContent("6acd67f1");
    expect(screen.getByRole("region", { name: "Train & evaluate" })).toHaveTextContent("962e98a1");
    expect(screen.getByTestId("training-train-promoted-note")).toHaveTextContent(`Train uses promoted dataset ${promotedId}, not the selected one`);
    expect(screen.queryByText(/Stage \d of/)).not.toBeInTheDocument();
    expect(screen.queryByText("Evaluate checkpoints")).not.toBeInTheDocument();
    expect(stage("Validate")).toHaveAttribute("data-state", "done");
    expect(stage("Canary")).toHaveAttribute("data-state", "next");
    expect(stage("Train")).toHaveAttribute("data-state", "done");
    expect(screen.getByRole("button", { name: "Start train" })).toBeDisabled();
  });

  it("expands stage explanations and dispatches the existing actions", () => {
    const onAction = mount();
    const canary = within(stage("Canary"));
    fireEvent.click(canary.getByText("How it works"));
    expect(canary.getByText("What it does")).toBeVisible();
    expect(canary.getByText("Reads")).toBeVisible();
    expect(canary.getByText("Writes")).toBeVisible();
    expect(canary.getByText("Latest result")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Start canary" }));
    expect(onAction).toHaveBeenCalledWith({ kind: "job", job: "canary" });
    fireEvent.click(screen.getByRole("button", { name: "Generate data" }));
    expect(onAction).toHaveBeenCalledWith({ kind: "job", job: "generate" });
    fireEvent.click(screen.getByRole("button", { name: "Wake train" }));
    expect(onAction).toHaveBeenCalledWith({ kind: "job", job: "wake-train" });
  });

  it("embeds running progress and preserves the busy guard", () => {
    mount({ state: "train:lfm", device: "gpu", run: { kind: "lfm-train", running: true,
      external: false, started: "", log_tail: [], progress: { step: 12, total: 100, loss: 0.5, eta_s: 120 } } }, null, true);
    expect(stage("Train")).toHaveAttribute("data-state", "active");
    expect(within(stage("Train")).getByText("Step 12 of 100 · loss 0.5000 · ETA ~2 min")).toBeVisible();
    expect(screen.getByLabelText("Train stage progress")).toHaveAttribute("value", "12");
    expect(screen.getByRole("button", { name: "Start canary" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Generate data" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Wake train" })).toBeDisabled();
  });

  it("shows the served model in the GPU row without a separate runtime card", () => {
    mount({}, { evals: [], champion: { run_id: promotedId, checkpoint: "/models/checkpoint-3000", pass_rates: { promotion: 0.7917, context: 1 } } });
    expect(screen.getByText("Serving Writer (llama.cpp)")).toBeVisible();
    expect(within(screen.getByLabelText("Hardware now")).getByText("writer.gguf")).toBeVisible();
    expect(screen.queryByRole("complementary", { name: "Runtime and serving" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Champion" })).toHaveTextContent("checkpoint-3000");
    expect(screen.getByRole("meter", { name: "Champion promotion pass rate" })).toHaveAttribute("value", "0.7917");
    expect(screen.getByRole("meter", { name: "Champion context pass rate" })).toHaveAttribute("value", "1");
  });

  it("labels sweep and experiment points on focus and tap, with a champion reference", () => {
    const row = { id: "sweep", group: `${promotedId}-select-checkpoint-3000-promotion-20261018T180653Z`, checkpoint_label: "checkpoint-3000",
      kind: "checkpoint" as const, suite: "promotion", pass_rate: 0.7917, gate_label: null, created_at: null,
      total: 120, passed: 95, promotion_passed: null, failures: [], category_pass_rate: {} };
    mount({}, { champion: { run_id: promotedId, checkpoint: "/models/checkpoint-3000", pass_rates: { promotion: 0.7917 } },
      evals: [row, { ...row, id: "experiment", group: "v13-promotion", pass_rate: 0.7 }] });
    const sweep = within(screen.getByRole("figure", { name: "Checkpoint sweep" }));
    const point = sweep.getByRole("button", { name: /checkpoint-3000.*Best.*Champion/ });
    fireEvent.focus(point);
    expect(point).toHaveAttribute("aria-pressed", "true");
    expect(sweep.getByText("checkpoint-3000 · 79.2% · Best · Champion")).toBeVisible();
    expect(sweep.getByRole("img", { name: `${row.group} · checkpoint-3000 · 79.2%` })).toBeInTheDocument();
    const experiments = within(screen.getByRole("figure", { name: "Experiment scores" }));
    fireEvent.click(experiments.getByRole("button", { name: /v13-promotion/ }));
    expect(experiments.getByText("v13-promotion · checkpoint-3000 · 70.0%", { selector: "p" })).toBeVisible();
    expect(experiments.getByRole("img", { name: "v13-promotion · checkpoint-3000 · 70.0%" })).toBeInTheDocument();
    expect(experiments.getByText("Champion · 79.2% — reference")).toBeVisible();
    expect(screen.queryByText("Evaluate checkpoints")).not.toBeInTheDocument();
  });

  it.each([false, true])("blocks commands for a different selected dataset when eval passed is %s", (finalPassed) => {
    const onAction = vi.fn();
    const records = [datasets[0], { ...datasets[1], final_eval_passed: finalPassed }];
    const currentStatus = status({ checkpoint_eval: false });
    const view = render(<TrainingPipeline status={currentStatus} datasets={records} evals={null}
      championRunId={null} jobsDisabled={false} onAction={onAction} />);
    const label = finalPassed ? "Start promote model" : "Start eval";
    expect(screen.getByRole("button", { name: label })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: label }));
    expect(onAction).not.toHaveBeenCalled();
    for (const name of ["Eval", "Promote model"]) {
      expect(within(stage(name)).getByText("Runs on the selected dataset (6acd67f1). Select 962e98a1 in Datasets to run it here.")).toBeVisible();
    }
    view.rerender(<TrainingPipeline status={{ ...currentStatus, pipeline: { ...currentStatus.pipeline!, run_id: promotedId } }}
      datasets={records} evals={null} championRunId={null} jobsDisabled={false} onAction={onAction} />);
    expect(screen.getByRole("button", { name: label })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: label }));
    expect(onAction).toHaveBeenCalledWith({ kind: "job", job: finalPassed ? "promote-model" : "lfm-eval" });
    expect(within(stage("Eval")).getByText("Score the trained run's checkpoint on the promotion suite (the promotion gate).")).toBeVisible();
  });

  it("places live evals in the promoted lane or experiments", () => {
    mount({ eval_progress: [promotedId + "-select-checkpoint", "adhoc"].map((group) => ({
      group, checkpoint_label: "checkpoint-3200", labels: ["promotion"], done: [], current: "promotion", elapsed_s: 1,
    })) });
    expect(within(screen.getByRole("region", { name: "Train & evaluate" })).getByText(`${promotedId}-select-checkpoint · checkpoint-3200`)).toBeVisible();
    expect(within(screen.getByRole("region", { name: "Experiments" })).getByText("adhoc · checkpoint-3200")).toBeVisible();
    expect(screen.getByText("2 running evals")).toBeVisible();
  });
});
