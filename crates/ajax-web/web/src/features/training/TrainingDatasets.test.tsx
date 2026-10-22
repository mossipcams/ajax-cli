import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@/test/testing-library-shim";
import TrainingPage from "./TrainingPage";
import type { TrainingStatus } from "./trainingApi";
import * as trainingApi from "./trainingApi";

const fetchDatasets = vi.spyOn(trainingApi, "fetchTrainingDatasets");
const fetchEvals = vi.spyOn(trainingApi, "fetchTrainingEvals");
const selectDataset = vi.spyOn(trainingApi, "selectTrainingDataset");

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

function datasetsPayload(
  datasets: Array<{
    run_id: string;
    current: boolean;
    promoted: boolean;
    size_bytes: number;
    validated: boolean;
    canary_passed: boolean;
    dataset_promoted: boolean;
    trained: boolean;
    final_eval_passed: boolean;
  }>,
  promotedRunId: string | null = null,
  promotedRecordPresent = false,
) {
  return {
    datasets,
    promoted_run_id: promotedRunId,
    promoted_record_present: promotedRecordPresent,
  };
}

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
  fetchDatasets.mockResolvedValue(
    datasetsPayload([
      {
        run_id: "run-1",
        current: true,
        promoted: false,
        size_bytes: 1024,
        validated: true,
        canary_passed: false,
        dataset_promoted: false,
        trained: false,
        final_eval_passed: false,
      },
      {
        run_id: "run-2",
        current: false,
        promoted: true,
        size_bytes: 4096,
        validated: true,
        canary_passed: true,
        dataset_promoted: true,
        trained: true,
        final_eval_passed: true,
      },
    ]),
  );
  fetchEvals.mockResolvedValue({ evals: [], champion: null });
  selectDataset.mockResolvedValue();
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

describe("TrainingDatasets", () => {
  it("disables Select and starts while eval progress is non-empty", async () => {
    const status = baseStatus({
      pipeline,
      eval_progress: [{
        group: "run-1", checkpoint_label: "checkpoint-600", labels: ["core"],
        done: [], current: "core", elapsed_s: 10,
      }],
    });
    mockedUseTrainingStatus.mockReturnValue({ status, error: null, refresh: refreshStatus });
    const view = mountPage();
    expect(await screen.findByRole("button", { name: "Select" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Start canary" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Generate data" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Wake train" })).toBeDisabled();
    mockedUseTrainingStatus.mockReturnValue({
      status: { ...status, eval_progress: [] }, error: null, refresh: refreshStatus,
    });
    view.rerender(<TrainingPage onBack={vi.fn()} />);
    expect(await screen.findByRole("button", { name: "Select" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Start canary" })).toBeEnabled();
  });

  it("disables dataset Select for an external run", async () => {
    mockedUseTrainingStatus.mockReturnValue({
      status: baseStatus({
        run: { kind: "Eval", started: "", running: true, external: true, progress: null, log_tail: [] },
      }),
      error: null, refresh: refreshStatus,
    });
    mountPage();
    expect(await screen.findByRole("button", { name: "Select" })).toBeDisabled();
  });

  it("disables a pending Select confirmation when an eval starts", async () => {
    const view = mountPage();
    fireEvent.click(await screen.findByRole("button", { name: "Select" }));
    expect(screen.getByRole("button", { name: "Confirm" })).toBeEnabled();
    mockedUseTrainingStatus.mockReturnValue({
      status: baseStatus({
        eval_progress: [{
          group: "run-1", checkpoint_label: "final", labels: ["core"],
          done: [], current: "core", elapsed_s: null,
        }],
      }),
      error: null, refresh: refreshStatus,
    });
    view.rerender(<TrainingPage onBack={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Confirm" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(selectDataset).not.toHaveBeenCalled();
  });

  it("renders datasets and selects a non-current row after confirm", async () => {
    mountPage();
    expect(await screen.findByText("run-2")).toBeTruthy();
    const selectButtons = await screen.findAllByRole("button", { name: "Select" });
    fireEvent.click(selectButtons[0]);
    fireEvent.click(await screen.findByRole("button", { name: "Confirm" }));
    await waitFor(() =>
      expect(selectDataset).toHaveBeenCalledWith("run-2"),
    );
  });

  it("disables dataset Select while a run is running", async () => {
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
    const selectButtons = await screen.findAllByRole("button", { name: "Select" });
    expect(selectButtons[0]).toBeDisabled();
  });

  it("ignores stale datasets responses when a newer refresh completes first", async () => {
    let resolveStale: (value: ReturnType<typeof datasetsPayload>) => void = () => {};
    fetchDatasets
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveStale = resolve;
          }),
      )
      .mockResolvedValue(
        datasetsPayload(
          [
            {
              run_id: "run-fresh",
              current: true,
              promoted: false,
              size_bytes: 2,
              validated: false,
              canary_passed: false,
              dataset_promoted: false,
              trained: false,
              final_eval_passed: false,
            },
          ],
          "run-fresh",
        ),
      );
    const view = mountPage();
    await waitFor(() => expect(fetchDatasets).toHaveBeenCalledTimes(1));
    vi.advanceTimersByTime(15000);
    await waitFor(() => expect(fetchDatasets).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("run-fresh")).toBeTruthy();
    resolveStale(
      datasetsPayload(
        [
          {
            run_id: "run-stale",
            current: true,
            promoted: false,
            size_bytes: 1,
            validated: false,
            canary_passed: false,
            dataset_promoted: false,
            trained: false,
            final_eval_passed: false,
          },
        ],
        "run-stale",
      ),
    );
    await waitFor(() => {
      expect(screen.getByText("run-fresh")).toBeTruthy();
      expect(screen.queryByText("run-stale")).toBeNull();
    });
    view.unmount();
  });
});
