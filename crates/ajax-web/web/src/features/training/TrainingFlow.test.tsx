import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@/test/testing-library-shim";
import TrainingPage from "./TrainingPage";
import type { TrainingStatus } from "./trainingApi";
import * as trainingApi from "./trainingApi";

const fetchDatasets = vi.spyOn(trainingApi, "fetchTrainingDatasets");
const fetchEvals = vi.spyOn(trainingApi, "fetchTrainingEvals");

const refreshStatus = vi.fn();

function baseStatus(overrides: Partial<TrainingStatus> = {}): TrainingStatus {
  return {
    ok: true,
    state: "idle",
    runtime_up: true,
    active_profile: "atomic",
    profiles: ["atomic"],
    run: null,
    generation: null,
    profile_details: null,
    device: null,
    pipeline: null,
    checkpoint_eval: false,
    eval_progress: [],
    ...overrides,
  };
}

const pipeline = {
  run_id: "run-1",
  validated: true,
  canary_passed: true,
  dataset_promoted: true,
  trained: false,
  final_eval_passed: false,
  promoted_run_id: "run-1",
  promoted_record_present: true,
};

vi.mock("./useTrainingStatus", () => ({
  useTrainingStatus: vi.fn(() => ({
    status: baseStatus(),
    error: null,
    refresh: refreshStatus,
  })),
}));

import { useTrainingStatus } from "./useTrainingStatus";

const mockedUseTrainingStatus = vi.mocked(useTrainingStatus);

function useStatus(status: TrainingStatus): void {
  mockedUseTrainingStatus.mockImplementation(() => ({
    status,
    error: null,
    refresh: refreshStatus,
  }));
}

function stepFor(label: string): HTMLElement {
  const step = screen
    .getAllByTestId("training-pipeline-step")
    .find((candidate) => within(candidate).queryByText(label, { exact: true }) !== null);
  if (!step) throw new Error(`no pipeline step for ${label}`);
  return step;
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  refreshStatus.mockReset();
  fetchDatasets.mockResolvedValue({ datasets: [], promoted_run_id: null, promoted_record_present: false });
  fetchEvals.mockResolvedValue({ evals: [], champion: null });
  useStatus(baseStatus({ pipeline, device: "gpu" }));
});

afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("TrainingFlow", () => {
  it("marks Train active with step and loss text while lfm-train runs", async () => {
    useStatus(
      baseStatus({
        pipeline: { ...pipeline, trained: false },
        device: "gpu",
        run: {
          kind: "lfm-train",
          started: new Date().toISOString(),
          running: true,
          external: false,
          progress: { step: 12, total: 100, loss: 0.5, eta_s: 120 },
          log_tail: [],
        },
      }),
    );
    render(<TrainingPage onBack={vi.fn()} />);
    const train = await waitForStep("Train");
    expect(train).toHaveAttribute("data-state", "active");
    expect(within(train).getByText("Step 12 of 100 · loss 0.5000 · ETA ~2 min")).toBeTruthy();
    expect(within(train).getByLabelText("Train stage progress")).toHaveAttribute("max", "100");
  });

  it("marks Generate active with rows text while generation runs", async () => {
    useStatus(
      baseStatus({
        pipeline,
        device: "gpu",
        generation: { rows: 40, target: 100, phase: "writing", running: true },
      }),
    );
    render(<TrainingPage onBack={vi.fn()} />);
    const generate = await waitForStep("Generate");
    expect(generate).toHaveAttribute("data-state", "active");
    expect(within(generate).getByText("Rows 40 of 100 · writing")).toBeTruthy();
    expect(within(generate).getByLabelText("Generate stage progress")).toHaveAttribute("value", "40");
  });

  it("keeps later stages pending while a job runs", async () => {
    useStatus(
      baseStatus({
        pipeline: { ...pipeline, trained: false },
        device: "gpu",
        run: {
          kind: "lfm-train",
          started: new Date().toISOString(),
          running: true,
          external: false,
          progress: { step: 3, total: 10, loss: 0.9, eta_s: null },
          log_tail: [],
        },
      }),
    );
    render(<TrainingPage onBack={vi.fn()} />);
    const evalStep = await waitForStep("Eval");
    expect(evalStep).toHaveAttribute("data-state", "pending");
    expect(screen.queryByRole("button", { name: "Start eval" })).toBeNull();
  });

  it("renders the promoted dataset artifact on the connector", async () => {
    render(<TrainingPage onBack={vi.fn()} />);
    expect(await screen.findByText("promoted dataset")).toBeTruthy();
  });

  it("shows the stage position in the flow header", async () => {
    useStatus(
      baseStatus({
        pipeline: { ...pipeline, canary_passed: false, dataset_promoted: false, promoted_run_id: null },
        device: "gpu",
      }),
    );
    render(<TrainingPage onBack={vi.fn()} />);
    expect(await screen.findByTestId("training-flow-position")).toHaveTextContent("Stage 3 of 7 · Canary");
  });
});

async function waitForStep(label: string): Promise<HTMLElement> {
  await screen.findAllByTestId("training-pipeline-step");
  return stepFor(label);
}
