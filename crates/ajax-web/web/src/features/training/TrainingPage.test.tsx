import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@/test/testing-library-shim";
import TrainingPage from "./TrainingPage";
import type { TrainingStatus } from "./trainingApi";
import * as trainingApi from "./trainingApi";

const fetchDatasets = vi.spyOn(trainingApi, "fetchTrainingDatasets");
const fetchEvals = vi.spyOn(trainingApi, "fetchTrainingEvals");
const stopTraining = vi.spyOn(trainingApi, "stopTraining");

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
  stopTraining.mockResolvedValue();
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

describe("TrainingPage", () => {
  it.each([
    ["lfm-eval", "Unsloth eval"],
    ["Eval", "Eval"],
  ])("shows an external %s run with its label and no Stop", async (kind, label) => {
    mockedUseTrainingStatus.mockReturnValue({
      status: baseStatus({
        device: "cpu", pipeline,
        run: { kind, started: "", running: true, external: true, progress: null, log_tail: [] },
      }),
      error: null,
      refresh: refreshStatus,
    });
    mountPage();
    expect(await screen.findByTestId("training-page-run")).toHaveTextContent(`${label} running`);
    expect(screen.getByText(/started outside Ajax/)).toHaveClass("training-muted");
    expect(screen.getByTestId("training-device")).toHaveTextContent("CPU");
    expect(screen.queryByRole("button", { name: "Stop" })).toBeNull();
    expect(screen.getByRole("button", { name: "Generate data" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Start canary" })).toBeDisabled();
  });

  it("shows GPU and CPU device badges from status", async () => {
    const view = mountPage();
    expect(await screen.findByTestId("training-device")).toHaveTextContent("GPU");
    view.unmount();

    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({ device: "cpu" }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    expect(await screen.findByTestId("training-device")).toHaveTextContent("CPU");
  });

  it("calls stopTraining once when Confirm is double-clicked", async () => {
    let resolveStop: () => void = () => {};
    stopTraining.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          resolveStop = resolve;
        }),
    );
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        pipeline,
        device: "gpu",
        run: {
          kind: "canary",
          started: "2026-01-01T00:00:00Z",
          running: true,
          external: false,
          progress: null,
          log_tail: [],
        },
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    const confirm = await screen.findByRole("button", { name: "Confirm" });
    fireEvent.click(confirm);
    fireEvent.click(confirm);
    expect(stopTraining).toHaveBeenCalledTimes(1);
    resolveStop();
    await waitFor(() => expect(stopTraining).toHaveBeenCalledTimes(1));
  });

  it("clears the catalog polling interval on unmount", async () => {
    const setIntervalSpy = vi.spyOn(window, "setInterval");
    const clearIntervalSpy = vi.spyOn(window, "clearInterval");
    const view = mountPage();
    await screen.findByTestId("training-page");
    expect(setIntervalSpy.mock.calls.some((call) => call[1] === 15000)).toBe(true);
    const clearsBefore = clearIntervalSpy.mock.calls.length;
    view.unmount();
    expect(clearIntervalSpy.mock.calls.length).toBeGreaterThan(clearsBefore);
    setIntervalSpy.mockRestore();
    clearIntervalSpy.mockRestore();
  });
});
