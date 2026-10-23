import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@/test/testing-library-shim";
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
  canary_passed: false,
  dataset_promoted: false,
  trained: false,
  final_eval_passed: false,
  promoted_run_id: null as string | null,
  promoted_record_present: false,
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

function mountPage(): ReturnType<typeof render> {
  return render(<TrainingPage onBack={vi.fn()} />);
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  refreshStatus.mockReset();
  fetchDatasets.mockResolvedValue({ datasets: [], promoted_run_id: null, promoted_record_present: false });
  fetchEvals.mockResolvedValue({ evals: [], champion: null });
  mockedUseTrainingStatus.mockImplementation(() => ({
    status: baseStatus({ pipeline, device: "gpu" }),
    error: null,
    refresh: refreshStatus,
  }));
});

afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("TrainingPipeline", () => {
  it("marks pipeline steps done and next from pipeline flags", async () => {
    mountPage();
    const steps = await screen.findAllByTestId("training-pipeline-step");
    const validate = steps.find((step) => step.textContent?.includes("Validate"));
    const canary = steps.find((step) => step.textContent?.includes("Canary"));
    expect(validate).toHaveAttribute("data-state", "done");
    expect(canary).toHaveAttribute("data-state", "next");
  });

  it("shows Train uses promoted dataset note when promoted id differs from selected", async () => {
    const promoted = "20260102T120000Z-fedcba98";
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        pipeline: {
          ...pipeline,
          run_id: "run-1",
          canary_passed: true,
          dataset_promoted: true,
          trained: false,
          promoted_run_id: promoted,
          promoted_record_present: true,
        },
        device: "gpu",
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    expect(await screen.findByTestId("training-train-promoted-note")).toHaveTextContent(
      `Train uses promoted dataset ${promoted}, not the selected one`,
    );
  });

  it("disables Train when promoted record is missing from checkout", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        pipeline: {
          ...pipeline,
          canary_passed: true,
          dataset_promoted: true,
          trained: false,
          promoted_run_id: "20260102T120000Z-fedcba98",
          promoted_record_present: false,
        },
        device: "gpu",
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    expect(await screen.findByTestId("training-train-missing-promoted")).toBeTruthy();
    const trainStep = (await screen.findAllByTestId("training-pipeline-step")).find((step) =>
      step.textContent?.includes("Train"),
    );
    expect(trainStep?.querySelector("button")).toBeDisabled();
  });

  it("shows Promote a dataset first when no promoted_run_id", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        pipeline: {
          ...pipeline,
          canary_passed: true,
          dataset_promoted: true,
          trained: false,
          promoted_run_id: null,
          promoted_record_present: false,
        },
        device: "gpu",
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    expect(await screen.findByTestId("training-train-hint")).toHaveTextContent(
      "Promote a dataset first",
    );
    const trainStep = (await screen.findAllByTestId("training-pipeline-step")).find((step) =>
      step.textContent?.includes("Train"),
    );
    expect(trainStep?.querySelector("button")).toBeDisabled();
  });

  it("shows Evaluate checkpoints only when host supports it and Train is complete", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({ pipeline: { ...pipeline, trained: false }, device: "gpu" }),
      error: null,
      refresh: refreshStatus,
    }));
    const view = mountPage();
    await screen.findByTestId("training-pipeline");
    expect(screen.queryByText("Evaluate checkpoints")).toBeNull();
    view.unmount();

    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        pipeline: {
          ...pipeline,
          trained: true,
          dataset_promoted: true,
          promoted_run_id: "20260101T120000Z-abcdef12",
          promoted_record_present: true,
        },
        checkpoint_eval: false,
        device: "gpu",
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    await screen.findByTestId("training-pipeline");
    expect(screen.queryByText("Evaluate checkpoints")).toBeNull();
  });

  it("shows Evaluate checkpoints when checkpoint_eval and trained are both true", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        pipeline: {
          ...pipeline,
          trained: true,
          dataset_promoted: true,
          promoted_run_id: "20260101T120000Z-abcdef12",
          promoted_record_present: true,
        },
        checkpoint_eval: true,
        device: "gpu",
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    expect(await screen.findByText("Evaluate checkpoints")).toBeTruthy();
  });
});
