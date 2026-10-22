import { afterEach, describe, expect, it, vi } from "vitest";
import {
  fetchTrainingDatasets,
  fetchTrainingEvals,
  fetchTrainingStatus,
} from "./trainingApi";

type FetchMock = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

function mockFetch(impl: FetchMock) {
  vi.stubGlobal("fetch", vi.fn(impl));
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("fetchTrainingStatus", () => {
  it("parses external runs and defaults external to false", async () => {
    for (const external of [true, false, undefined, "true"]) {
      mockFetch(() => Promise.resolve(json({
        ok: true, state: "idle", run: { kind: "Eval", running: true, external },
      })));
      const status = await fetchTrainingStatus();
      expect(status.run?.external).toBe(external === true);
      expect(status.run?.kind).toBe("Eval");
    }
  });

  it("parses eval progress and drops malformed entries", async () => {
    const progress = {
      checkpoint_label: "checkpoint-600", group: "run-1",
      labels: ["core", "tools"], done: ["core"], current: "tools", elapsed_s: 90,
    };
    const waiting = { ...progress, current: null, elapsed_s: null };
    mockFetch(() => Promise.resolve(json({
      ok: true, state: "idle", eval_progress: [progress, waiting, null, {},
        { ...progress, checkpoint_label: 42 }, { ...progress, group: false },
        { ...progress, labels: "core" }, { ...progress, labels: ["core", 1] },
        { ...progress, labels: [] }, { ...progress, done: [false] },
        { ...progress, done: null }, { ...progress, current: 1 },
        { ...progress, elapsed_s: "90" }, { ...progress, elapsed_s: -1 },
      ],
    })));
    expect((await fetchTrainingStatus()).eval_progress).toEqual([progress, waiting]);
  });

  it("defaults missing or non-array eval progress to an empty array", async () => {
    for (const eval_progress of [undefined, null, {}, "running"]) {
      mockFetch(() => Promise.resolve(json({ ok: true, state: "idle", eval_progress })));
      expect((await fetchTrainingStatus()).eval_progress).toEqual([]);
    }
  });

  it("parses device and pipeline on status payloads", async () => {
    mockFetch((input) => {
      if (String(input).includes("/api/training/status")) {
        return Promise.resolve(
          json({
            ok: true,
            state: "idle",
            runtime_up: true,
            active_profile: "atomic",
            profiles: ["atomic"],
            run: null,
            generation: null,
            profile_details: null,
            device: "gpu",
            pipeline: {
              run_id: "run-42",
              validated: true,
              canary_passed: false,
              dataset_promoted: false,
              trained: false,
              final_eval_passed: false,
              promoted_run_id: "20260101T120000Z-abcdef12",
              promoted_record_present: true,
            },
          }),
        );
      }
      return Promise.reject(new Error(`unmocked ${String(input)}`));
    });

    const status = await fetchTrainingStatus();
    expect(status.device).toBe("gpu");
    expect(status.pipeline).toEqual({
      run_id: "run-42",
      validated: true,
      canary_passed: false,
      dataset_promoted: false,
      trained: false,
      final_eval_passed: false,
      promoted_run_id: "20260101T120000Z-abcdef12",
      promoted_record_present: true,
    });
  });

  it("defaults missing pipeline promotion fields to null and false", async () => {
    mockFetch(() =>
      Promise.resolve(
        json({
          ok: true,
          state: "idle",
          runtime_up: true,
          active_profile: null,
          profiles: [],
          run: null,
          generation: null,
          profile_details: null,
          pipeline: {
            run_id: "run-1",
            validated: false,
            canary_passed: false,
            dataset_promoted: false,
            trained: false,
            final_eval_passed: false,
          },
        }),
      ),
    );

    const status = await fetchTrainingStatus();
    expect(status.pipeline?.promoted_run_id).toBeNull();
    expect(status.pipeline?.promoted_record_present).toBe(false);
  });

  it("treats missing device and pipeline as null", async () => {
    mockFetch(() =>
      Promise.resolve(
        json({
          ok: true,
          state: "idle",
          runtime_up: false,
          active_profile: null,
          profiles: [],
          run: null,
          generation: null,
          profile_details: null,
        }),
      ),
    );

    const status = await fetchTrainingStatus();
    expect(status.device).toBeNull();
    expect(status.pipeline).toBeNull();
  });

  it("parses checkpoint_eval and defaults to false when missing or non-boolean", async () => {
    mockFetch((input) => {
      if (String(input).includes("/api/training/status")) {
        return Promise.resolve(
          json({
            ok: true,
            state: "idle",
            runtime_up: true,
            active_profile: null,
            profiles: [],
            run: null,
            generation: null,
            profile_details: null,
            checkpoint_eval: true,
          }),
        );
      }
      return Promise.reject(new Error(`unmocked ${String(input)}`));
    });
    expect((await fetchTrainingStatus()).checkpoint_eval).toBe(true);

    mockFetch(() =>
      Promise.resolve(
        json({
          ok: true,
          state: "idle",
          runtime_up: false,
          active_profile: null,
          profiles: [],
          run: null,
          generation: null,
          profile_details: null,
        }),
      ),
    );
    expect((await fetchTrainingStatus()).checkpoint_eval).toBe(false);

    mockFetch(() =>
      Promise.resolve(
        json({
          ok: true,
          state: "idle",
          runtime_up: false,
          active_profile: null,
          profiles: [],
          run: null,
          generation: null,
          profile_details: null,
          checkpoint_eval: "yes",
        }),
      ),
    );
    expect((await fetchTrainingStatus()).checkpoint_eval).toBe(false);
  });
});

describe("fetchTrainingDatasets", () => {
  it("parses dataset rows defensively", async () => {
    mockFetch((input) => {
      if (String(input).includes("/api/training/datasets")) {
        return Promise.resolve(
          json({
            datasets: [
              {
                run_id: "run-a",
                current: true,
                promoted: false,
                size_bytes: 2048,
                validated: true,
                canary_passed: true,
                dataset_promoted: false,
                trained: false,
                final_eval_passed: false,
              },
              { not_a_run: true },
            ],
          }),
        );
      }
      return Promise.reject(new Error(`unmocked ${String(input)}`));
    });

    const datasets = await fetchTrainingDatasets();
    expect(datasets.datasets).toHaveLength(1);
    expect(datasets.datasets[0]?.run_id).toBe("run-a");
    expect(datasets.datasets[0]?.size_bytes).toBe(2048);
    expect(datasets.promoted_run_id).toBeNull();
    expect(datasets.promoted_record_present).toBe(false);
  });

  it("parses top-level promoted_run_id and promoted_record_present", async () => {
    mockFetch((input) => {
      if (String(input).includes("/api/training/datasets")) {
        return Promise.resolve(
          json({
            datasets: [],
            promoted_run_id: "20260102T120000Z-fedcba98",
            promoted_record_present: true,
          }),
        );
      }
      return Promise.reject(new Error(`unmocked ${String(input)}`));
    });

    const datasets = await fetchTrainingDatasets();
    expect(datasets.promoted_run_id).toBe("20260102T120000Z-fedcba98");
    expect(datasets.promoted_record_present).toBe(true);
  });

  it("raises incompatible on malformed datasets payload", async () => {
    mockFetch(() => Promise.resolve(json({ nope: true })));
    await expect(fetchTrainingDatasets()).rejects.toMatchObject({ kind: "incompatible" });
  });
});

describe("fetchTrainingEvals", () => {
  it("parses eval rows and champion pass_rates", async () => {
    mockFetch((input) => {
      if (String(input).includes("/api/training/evals")) {
        return Promise.resolve(
          json({
            evals: [
              {
                id: "e1",
                group: "g",
                checkpoint_label: "ckpt-1",
                kind: "checkpoint",
                suite: "core",
                gate_label: null,
                created_at: "2026-01-02T00:00:00Z",
                total: 10,
                passed: 9,
                pass_rate: 0.9,
                promotion_passed: true,
                failures: [],
                category_pass_rate: { math: 0.8 },
              },
              { id: 2, kind: "nope" },
            ],
            champion: {
              run_id: "run-champ",
              checkpoint: "final",
              pass_rates: { core: 0.85 },
            },
          }),
        );
      }
      return Promise.reject(new Error(`unmocked ${String(input)}`));
    });

    const result = await fetchTrainingEvals();
    expect(result.evals).toHaveLength(1);
    expect(result.evals[0]?.kind).toBe("checkpoint");
    expect(result.evals[0]?.category_pass_rate).toEqual({ math: 0.8 });
    expect(result.champion?.pass_rates).toEqual({ core: 0.85 });
  });

  it("raises incompatible on malformed evals payload", async () => {
    mockFetch(() => Promise.resolve(json({ evals: "nope" })));
    await expect(fetchTrainingEvals()).rejects.toMatchObject({ kind: "incompatible" });
  });
});
