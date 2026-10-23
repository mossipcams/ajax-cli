import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@/test/testing-library-shim";
import TrainingPage from "./TrainingPage";
import type { TrainingStatus } from "./trainingApi";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import * as trainingApi from "./trainingApi";
import { formatElapsed, formatEta } from "./trainingPageModel";

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


function runningStatus(logTail: string[]): TrainingStatus {
  return baseStatus({
    pipeline,
    device: "gpu",
    run: { kind: "canary", started: "", running: true, external: false, progress: null, log_tail: logTail },
  });
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


  it("keeps confirm and action error together in the sticky dock", async () => {
    let rejectStop: (err: unknown) => void = () => {};
    stopTraining.mockReturnValue(
      new Promise<void>((_resolve, reject) => {
        rejectStop = reject;
      }),
    );
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: runningStatus([]),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
    const confirm = await screen.findByRole("button", { name: "Confirm" });

    expect(screen.getByTestId("training-page-dock")).toContainElement(
      screen.getByTestId("training-page-confirm"),
    );

    fireEvent.click(confirm);
    rejectStop(new Error("boom"));
    const actionError = await screen.findByTestId("training-page-action-error");

    expect(screen.getByTestId("training-page-dock")).toContainElement(
      screen.getByTestId("training-page-confirm"),
    );
    expect(screen.getByTestId("training-page-dock")).toContainElement(actionError);
  });

  it("pins the log tail to the newest line while scrolled to the bottom", async () => {
    const setLog = (lines: string[]) => {
      mockedUseTrainingStatus.mockImplementation(() => ({
        status: runningStatus(lines),
        error: null,
        refresh: refreshStatus,
      }));
    };
    setLog(["one"]);
    const view = mountPage();
    const pre = (await screen.findByTestId("training-page-log")) as HTMLPreElement;
    Object.defineProperty(pre, "scrollHeight", { value: 200, configurable: true });
    Object.defineProperty(pre, "clientHeight", { value: 100, configurable: true });
    Object.defineProperty(pre, "scrollTop", { value: 0, configurable: true, writable: true });
    expect(pre.scrollTop).toBe(0);

    setLog(["one", "two"]);
    view.rerender(<TrainingPage onBack={vi.fn()} />);

    expect(pre.scrollTop).toBe(200);
  });

  it("leaves the log tail alone when the user has scrolled up", async () => {
    const setLog = (lines: string[]) => {
      mockedUseTrainingStatus.mockImplementation(() => ({
        status: runningStatus(lines),
        error: null,
        refresh: refreshStatus,
      }));
    };
    setLog(["one"]);
    const view = mountPage();
    const pre = (await screen.findByTestId("training-page-log")) as HTMLPreElement;
    Object.defineProperty(pre, "scrollHeight", { value: 200, configurable: true });
    Object.defineProperty(pre, "clientHeight", { value: 100, configurable: true });
    Object.defineProperty(pre, "scrollTop", { value: 50, configurable: true, writable: true });
    fireEvent.scroll(pre);

    setLog(["one", "two"]);
    view.rerender(<TrainingPage onBack={vi.fn()} />);

    expect(pre.scrollTop).toBe(50);
  });

  it("shows generation progress with rows and target", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        device: "gpu", pipeline,
        run: { kind: "generate", started: "", running: true, external: false, progress: null, log_tail: [] },
        generation: { rows: 5, target: 10, phase: "synthetic", running: true },
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    const block = await screen.findByTestId("training-page-generation");
    expect(block).toHaveTextContent("Rows 5 of 10 · synthetic");
    const progressEl = block.querySelector('progress[aria-label="Data generation progress"]') as HTMLProgressElement;
    expect(progressEl).not.toBeNull();
    expect(progressEl.max).toBe(10);
    expect(screen.queryByTestId("training-page-progress")).toBeNull();
  });

  it("shows the outside-Ajax line when generation runs without a run", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        device: "gpu", pipeline,
        generation: { rows: 2, target: null, phase: null, running: true },
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    expect(await screen.findByText("Data generation running · started outside Ajax")).toHaveClass("training-muted");
    expect(screen.getByTestId("training-page-generation")).toHaveTextContent("2 rows");
    expect(screen.queryByText("No run in progress.")).toBeNull();
  });

  it("shows a busy line above the pipeline when only an eval is running", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        device: "cpu", pipeline,
        eval_progress: [{ checkpoint_label: "step-100", group: "g1", labels: [], done: [], current: null, elapsed_s: null }],
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    expect(await screen.findByTestId("training-page-busy"))
      .toHaveTextContent("Jobs unavailable: Eval running: step-100");
  });

  it("shows no busy line when the host is idle", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({ device: "gpu", pipeline }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    await screen.findByTestId("training-device");
    expect(screen.queryByTestId("training-page-busy")).toBeNull();
  });
});

describe("formatEta", () => {
  it.each([
    [36000, "~10 h"],
    [36300, "~10 h 5 min"],
    [300, "~5 min"],
    [20, "~20s"],
  ])("formats %i seconds as %s", (seconds: number, expected: string) => {
    expect(formatEta(seconds)).toBe(expected);
  });
});

describe("formatElapsed", () => {
  const now = Date.parse("2026-01-15T12:00:00Z");

  it("formats a start 2 h 5 min ago as hours and minutes", () => {
    const started = new Date(now - (2 * 3600 + 5 * 60) * 1000).toISOString();
    expect(formatElapsed(started, now)).toBe("2 h 5 min");
  });

  it("returns null for a start less than 5 s ago", () => {
    const started = new Date(now - 3000).toISOString();
    expect(formatElapsed(started, now)).toBeNull();
  });

  it("returns null for an unparseable timestamp", () => {
    expect(formatElapsed("garbage", now)).toBeNull();
  });
});

describe("TrainingPage elapsed time", () => {
  it("shows elapsed time on the running job step line", async () => {
    mockedUseTrainingStatus.mockImplementation(() => ({
      status: baseStatus({
        device: "gpu", pipeline,
        run: { kind: "canary", started: new Date(Date.now() - 3600_000).toISOString(), running: true, external: false, progress: null, log_tail: [] },
      }),
      error: null,
      refresh: refreshStatus,
    }));
    mountPage();
    const line = await screen.findByText(/elapsed/);
    expect(line).toHaveClass("training-muted");
  });
});

describe("TrainingPage styles", () => {
  it("does not pad the page on top of route-scroll (#1402)", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const stylesSource = readOrderedStylesSource(join(here, "../.."));
    const block = stylesSource.match(/(?:^|\n)\.training-page\s*\{([^}]*)\}/)?.[1];
    expect(block).toBeDefined();
    expect(block).not.toMatch(/\bpadding\s*:/);
  });
});
