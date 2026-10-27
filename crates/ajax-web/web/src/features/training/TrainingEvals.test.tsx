import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@/test/testing-library-shim";
import TrainingPage from "./TrainingPage";
import TrainingEvalsSection from "./TrainingEvals";
import type { TrainingEval, TrainingEvals, TrainingStatus } from "./trainingApi";
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

const RUN_A = "20260101T120000Z-abcdef12";
const RUN_B = "20260102T120000Z-fedcba98";

const sampleEvals: TrainingEvals = {
  evals: [
    {
      id: "older",
      group: `${RUN_A}-select-checkpoint-600`,
      checkpoint_label: "ckpt-old",
      kind: "checkpoint",
      suite: "core",
      gate_label: null,
      created_at: "2026-01-01T12:00:00Z",
      total: 10,
      passed: 8,
      pass_rate: 0.8,
      promotion_passed: true,
      failures: [],
      category_pass_rate: {},
    },
    {
      id: "newer",
      group: `${RUN_B}-full`,
      checkpoint_label: "ckpt-new",
      kind: "final",
      suite: "core",
      gate_label: null,
      created_at: "2026-01-02T12:00:00Z",
      total: 10,
      passed: 9,
      pass_rate: 0.9,
      promotion_passed: false,
      failures: ["case-a"],
      category_pass_rate: { logic: 0.5 },
    },
  ],
  champion: {
    run_id: "run-champ",
    checkpoint: "/workspace/runs/run-champ/checkpoint-3000",
    pass_rates: { core: 0.85 },
  },
};

function makeEval(index: number, overrides: Partial<TrainingEval> = {}): TrainingEval {
  return {
    id: `eval-${index}`,
    group: `${RUN_B}-canary-eval`,
    checkpoint_label: null,
    kind: "final",
    suite: "core",
    gate_label: null,
    created_at: `2026-01-${String((index % 28) + 1).padStart(2, "0")}T12:00:00Z`,
    total: 10,
    passed: 9,
    pass_rate: 0.9,
    promotion_passed: null,
    failures: [],
    category_pass_rate: {},
    ...overrides,
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
  fetchDatasets.mockResolvedValue({ datasets: [], promoted_run_id: null, promoted_record_present: false });
  fetchEvals.mockResolvedValue(sampleEvals);
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

describe("TrainingEvals", () => {
  it("renders running eval cards with suite progress, done chips, and elapsed minutes", () => {
    const entry = {
      group: `${RUN_A}-select-checkpoint-600`, checkpoint_label: "checkpoint-600",
      labels: ["core", "logic", "tools"], done: ["core"], current: "logic", elapsed_s: 125,
    };
    const view = render(<TrainingEvalsSection evals={null} progress={[entry, {
      ...entry, checkpoint_label: "checkpoint-900", done: [], elapsed_s: null,
    }]} />);
    const cards = screen.getAllByTestId("training-eval-progress");
    expect(cards).toHaveLength(2);
    const card = within(cards[0]);
    expect(card.getByText("Running")).toBeTruthy();
    expect(card.getByText(RUN_A)).toBeTruthy();
    expect(card.getByText("select-checkpoint-600")).toHaveClass("training-muted");
    expect(card.getByText("· checkpoint-600")).toBeTruthy();
    expect(card.getByText("Suite 2 of 3: logic")).toBeTruthy();
    expect(card.getByRole("progressbar")).toHaveAttribute("value", "1");
    expect(card.getByRole("progressbar")).toHaveAttribute("max", "3");
    expect(card.getByText("core · done")).toHaveClass("training-tag");
    expect(card.getByText("logic", { exact: true })).not.toHaveTextContent("done");
    expect(card.getByText("2 min elapsed")).toBeTruthy();
    expect(within(cards[1]).queryByText(/min elapsed/)).toBeNull();
    view.rerender(<TrainingEvalsSection evals={null} progress={[{
      ...entry, done: entry.labels, current: null,
    }]} />);
    expect(screen.getByText("Suite 3 of 3: Waiting")).toBeTruthy();
    expect(screen.getByRole("progressbar")).toHaveAttribute("value", "3");
  });

  it("refreshes the catalog immediately when an eval progress entry disappears", async () => {
    const entry = {
      group: RUN_A, checkpoint_label: "checkpoint-600", labels: ["core", "tools"],
      done: [] as string[], current: "core", elapsed_s: 10,
    };
    const another = { ...entry, checkpoint_label: "checkpoint-900" };
    let status = baseStatus({ eval_progress: [entry, another] });
    mockedUseTrainingStatus.mockImplementation(() => ({ status, error: null, refresh: refreshStatus }));
    const view = mountPage();
    await screen.findAllByTestId("training-eval-row");
    expect(fetchEvals).toHaveBeenCalledTimes(1);
    status = { ...status, eval_progress: [another, { ...entry, done: ["core"], current: "tools" }] };
    view.rerender(<TrainingPage onBack={vi.fn()} />);
    expect(fetchEvals).toHaveBeenCalledTimes(1);
    fetchEvals.mockResolvedValue({ evals: [makeEval(99, { group: "finished-eval" })], champion: null });
    status = { ...status, eval_progress: [another, { ...entry, checkpoint_label: "checkpoint-1200" }] };
    view.rerender(<TrainingPage onBack={vi.fn()} />);
    expect(await screen.findByText("finished-eval")).toBeTruthy();
    expect(fetchEvals).toHaveBeenCalledTimes(2);
    status = { ...status, eval_progress: [] };
    view.rerender(<TrainingPage onBack={vi.fn()} />);
    await waitFor(() => expect(fetchEvals).toHaveBeenCalledTimes(3));
    expect(screen.queryByTestId("training-eval-progress")).toBeNull();
  });

  it("lists evals newest first with kind labels, filter, and champion delta", async () => {
    mountPage();
    const rows = await screen.findAllByTestId("training-eval-row");
    expect(rows[0]).toHaveTextContent(RUN_B);
    expect(rows[0]).toHaveTextContent("full");
    expect(rows[0]).not.toHaveTextContent("ckpt-new");
    expect(rows[0]).toHaveTextContent("Final");
    expect(rows[1]).toHaveTextContent("Checkpoint");
    expect(rows[1]).toHaveTextContent("ckpt-old");

    const deltas = await screen.findAllByTestId("training-eval-delta");
    expect(deltas[0]).toHaveTextContent("+5.0 pts");

    fireEvent.click(await screen.findByRole("button", { name: "Checkpoints" }));
    const filtered = await screen.findAllByTestId("training-eval-row");
    expect(filtered).toHaveLength(1);
    expect(filtered[0]).toHaveTextContent("Checkpoint");
  });

  it("shows ad-hoc for null suite and omits gate text when promotion_passed is null", async () => {
    fetchEvals.mockResolvedValue({
      evals: [
        makeEval(0, {
          suite: null,
          promotion_passed: null,
          created_at: "2026-02-01T12:00:00Z",
        }),
      ],
      champion: null,
    });
    mountPage();
    const row = await screen.findByTestId("training-eval-row");
    expect(row).toHaveTextContent("ad-hoc");
    expect(row).not.toHaveTextContent("No suite");
    expect(row).not.toHaveTextContent("Gate —");
    expect(row).not.toHaveTextContent("Gate pass");
  });

  it("paginates evals with Show more", async () => {
    const evals = Array.from({ length: 25 }, (_, index) =>
      makeEval(index, { created_at: `2026-03-${String(25 - index).padStart(2, "0")}T12:00:00Z` }),
    );
    fetchEvals.mockResolvedValue({ evals, champion: null });
    mountPage();
    expect(await screen.findAllByTestId("training-eval-row")).toHaveLength(20);
    fireEvent.click(await screen.findByTestId("training-eval-show-more"));
    expect(await screen.findAllByTestId("training-eval-row")).toHaveLength(25);
    expect(screen.queryByTestId("training-eval-show-more")).toBeNull();
  });

  it("shows champion checkpoint basename with full path in title", async () => {
    mountPage();
    const checkpoint = await screen.findByTestId("training-champion-checkpoint");
    expect(checkpoint).toHaveTextContent("checkpoint-3000");
    expect(checkpoint).toHaveAttribute("title", "/workspace/runs/run-champ/checkpoint-3000");
  });
});
