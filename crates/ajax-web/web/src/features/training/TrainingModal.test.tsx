import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@/test/testing-library-shim";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import TrainingModal from "./TrainingModal";
import * as trainingApi from "./trainingApi";

const statusRoute: { body: unknown; status?: number } = { body: {} };
let postResponse: { body: unknown; status?: number } = { body: {} };
const postCalls: Array<{ url: string; body: Record<string, unknown> }> = [];

vi.stubGlobal(
  "fetch",
  vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    if (method === "POST") {
      postCalls.push({ url, body: JSON.parse(String(init?.body)) });
      return jsonResponse(postResponse.body, postResponse.status);
    }
    if (url.includes("/api/training/status")) {
      return jsonResponse(statusRoute.body, statusRoute.status);
    }
    throw new Error(`unmocked route ${url}`);
  }),
);

function jsonResponse(body: unknown, status?: number): Response {
  return new Response(JSON.stringify(body), {
    status: status ?? 200,
    headers: { "content-type": "application/json" },
  });
}

const idleStatus = {
  ok: true,
  state: "idle",
  runtime_up: false,
  active_profile: null,
  profiles: ["atomic", "swift-1.5"],
  run: null,
  generation: null,
  profile_details: {
    atomic: { label: "Atomic", model: "qwen3-8b", serving: "ctx 122880, 2 slots" },
    "swift-1.5": { label: "Swift 1.5", model: "qwen3-4b", serving: "" },
  },
};

const onOpenChange = vi.fn();
function openModal(): void {
  render(<TrainingModal open onOpenChange={onOpenChange} />);
}

type FetchMock = (i: RequestInfo | URL, o?: RequestInit) => Promise<Response>;

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  postCalls.length = 0;
  statusRoute.body = idleStatus;
  statusRoute.status = undefined;
  postResponse = { body: { ok: true } };
});

afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
});

describe("TrainingModal", () => {

  it("locks the training modal as a vertical-only scroll owner (no horizontal drag pan)", () => {
    const css = readFileSync(
      join(dirname(fileURLToPath(import.meta.url)), "../../styles/settings.css"),
      "utf8",
    );
    const ruleBody = /(^|\n)\.training-modal\s*\{([^}]*)\}/.exec(css)?.[2] ?? "";
    expect(ruleBody, ".training-modal rule not found in styles/settings.css").not.toBe("");
    expect(ruleBody).toMatch(/overflow-x:\s*hidden/);
    expect(ruleBody).toMatch(/touch-action:\s*pan-y/);
    expect(ruleBody).toMatch(/overscroll-behavior:\s*contain/);
  });
  it("shows a skeleton while the first status fetch is pending, then the hero", async () => {
    const routeMock = globalThis.fetch as unknown as FetchMock;
    let settleStatus: (response: Response) => void = () => undefined;
    try {
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          if (String(input).includes("/status")) {
            return new Promise<Response>((resolve) => {
              settleStatus = resolve;
            });
          }
          return routeMock(input, init);
        }),
      );
      openModal();

      expect(screen.getByTestId("training-loading")).toBeTruthy();
      expect(screen.queryByRole("button", { name: "Unsloth train" })).toBeNull();

      await act(async () => {
        settleStatus(jsonResponse(idleStatus));
        vi.advanceTimersByTime(10);
      });
      expect(await screen.findByTestId("training-state")).toBeTruthy();
      expect(screen.queryByTestId("training-loading")).toBeNull();
    } finally {
      vi.stubGlobal("fetch", routeMock);
    }
  });

  it("renders the idle hero with an obvious primary action and no visible raw state", async () => {
    openModal();
    const hero = await screen.findByTestId("training-hero");
    expect(hero).toHaveTextContent("Idle");
    const raw = screen.getByTestId("training-raw-state");
    expect(raw).toHaveClass("training-sr-only");
    expect(raw).toHaveTextContent("idle");
    expect(hero).toHaveAttribute("title", "idle");

    const primary = screen.getAllByRole("button", { name: "Unsloth train" });
    expect(primary.length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Data generation" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Wake train" })).toBeTruthy();
    expect(hero).toHaveTextContent(/Start a job to begin/);
    expect(hero).toHaveTextContent("No run in progress.");
  });

  it("renders the serving profile with a stop action", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "atomic", runtime_up: true };
    openModal();
    expect(await screen.findByTestId("training-hero")).toHaveTextContent("Runtime ready");
    expect(screen.getByText("serving")).toBeTruthy();
    expect(screen.getByText("ctx 122880, 2 slots")).toBeTruthy();
    expect(
      screen.getAllByRole("button", { name: "Stop serving" }).length,
    ).toBeGreaterThan(0);
  });

  it("renders a training run with formatted stats and a Stop action beside them", async () => {
    statusRoute.body = {
      ...idleStatus,
      state: "train:lfm-train",
      active_profile: "unsloth",
      run: {
        kind: "lfm-train",
        started: "2026-07-19T08:00:00Z",
        running: true,
        progress: { step: 30, total: 100, loss: 1.2345, eta_s: 360 },
        log_tail: ["loss=1.2", "step 30"],
      },
    };
    openModal();
    const hero = await screen.findByTestId("training-hero");
    expect(hero).toHaveTextContent("Unsloth train running");
    expect(hero).toHaveAttribute("title", "train:lfm-train");
    expect(screen.getByTestId("training-raw-state")).toHaveClass(
      "training-sr-only",
    );

    const loss = screen.getByTestId("training-loss");
    expect(loss).toHaveTextContent("1.23");
    expect(hero).not.toHaveTextContent("1.2345");

    const eta = screen.getByTestId("training-eta");
    expect(eta).toHaveTextContent("~6 min");
    expect(eta.closest(".is-prominent")).toBeTruthy();

    const step = screen.getByTestId("training-step");
    expect(step).toHaveTextContent("step 30 / 100");
    expect(screen.getByRole("progressbar", { name: "lfm-train progress" })).toBeTruthy();

    const stop = screen.getByRole("button", { name: "Stop run" });
    expect(stop.closest(".training-hero-body")).toBeTruthy();

    const log = screen.getByTestId("training-log");
    expect(Array.from(log.querySelectorAll("li")).map((el) => el.textContent)).toEqual([
      "loss=1.2",
      "step 30",
    ]);
  });

  it("renders generation with target and rows-only when target is null", async () => {
    statusRoute.body = { ...idleStatus, state: "generate", generation: { rows: 40, target: 100, running: true } };
    openModal();
    expect(await screen.findByTestId("training-generation-rows")).toHaveTextContent("40 / 100");
    expect(screen.getByRole("progressbar", { name: "Data generation progress" })).toBeTruthy();

    statusRoute.body = { ...idleStatus, state: "generate", generation: { rows: 7, target: null, running: true } };
    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    expect(await screen.findByTestId("training-generation-rows")).toHaveTextContent("7 rows");
  });

  it("shows a live training job from the host state when no run object is sent", async () => {
    statusRoute.body = { ...idleStatus, state: "train:lfm 123 456 1700000000", run: null };
    openModal();
    expect(await screen.findByTestId("training-state")).toHaveTextContent(
      "Training in progress: lfm",
    );
    expect(screen.getByTestId("training-live-state")).toHaveTextContent(
      "No progress reported by the host yet.",
    );
    expect(screen.queryByText("No run in progress.")).toBeNull();
  });

  it("sends no mutating request without the confirm bar step", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "llama", profiles: ["llama", "unsloth"] };
    openModal();

    fireEvent.click(await screen.findByRole("button", { name: /unsloth/ }));
    expect(postCalls).toHaveLength(0);
    const confirm = await screen.findByTestId("training-confirm");
    expect(confirm).toHaveTextContent("Switch the active profile to unsloth?");

    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(postCalls).toHaveLength(1));
    expect(postCalls[0].url).toContain("/api/training/models/switch");
    expect(postCalls[0].body).toEqual({ profile: "unsloth", confirm: true });

    fireEvent.click(screen.getAllByRole("button", { name: "Unsloth train" })[1] ?? screen.getAllByRole("button", { name: "Unsloth train" })[0]);
    expect(postCalls).toHaveLength(1);
    const jobConfirm = await screen.findByTestId("training-confirm");
    expect(jobConfirm).toHaveTextContent("Start the Unsloth train job?");

    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(postCalls).toHaveLength(2));
    expect(postCalls[1].url).toContain("/api/training/start");
    expect(postCalls[1].body).toEqual({ job: "lfm-train", confirm: true });
  });

  it("disables profile switching while busy and says why", async () => {
    statusRoute.body = {
      ...idleStatus,
      state: "train:lfm-train",
      active_profile: "llama",
      profiles: ["llama", "unsloth"],
    };
    openModal();
    expect(await screen.findByRole("button", { name: /unsloth/ })).toBeDisabled();
    expect(
      screen.getByText(/Switching is unavailable while (a job is in progress|state is train:)/),
    ).toBeTruthy();
  });

  it("polls status while open and stops polling when closed", async () => {
    const fetchMock = vi.mocked(globalThis.fetch);
    let open = true;
    const { rerender } = render(<TrainingModal open={open} onOpenChange={onOpenChange} />);
    await screen.findByTestId("training-state");

    await act(async () => {
      vi.advanceTimersByTime(6000);
    });
    const whileOpen = statusCallCount();
    expect(whileOpen).toBeGreaterThanOrEqual(2);

    open = false;
    rerender(<TrainingModal open={open} onOpenChange={onOpenChange} />);
    await act(async () => {
      vi.advanceTimersByTime(9000);
    });
    expect(statusCallCount()).toBe(whileOpen);

    function statusCallCount(): number {
      return fetchMock.mock.calls.filter((call) => String(call[0]).includes("/status")).length;
    }
  });

  it("labels the in-flight action instead of silently disabling everything", async () => {
    const routeMock = globalThis.fetch as unknown as FetchMock;

    openModal();
    await screen.findByTestId("training-state");

    fireEvent.click(screen.getAllByRole("button", { name: "Unsloth train" })[0]);
    fireEvent.click(await screen.findByRole("button", { name: "Confirm" }));

    try {
      const busyButton = await screen.findByRole("button", { name: /Starting/ });
      expect(busyButton).toHaveAttribute("aria-busy", "true");
    } finally {
      vi.stubGlobal("fetch", routeMock);
    }
  });

  it("cancelling an armed profile switch sends no request", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "llama", profiles: ["llama", "unsloth"] };
    openModal();

    fireEvent.click(await screen.findByRole("button", { name: /unsloth/ }));
    const cancel = await screen.findByRole("button", { name: "Cancel" });
    fireEvent.click(cancel);

    expect(postCalls).toHaveLength(0);
    expect(screen.queryByTestId("training-confirm")).toBeNull();
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    expect(await screen.findByRole("button", { name: /unsloth/ })).toBeTruthy();
  });

  it("does not surface a stale action error when the dialog closes while a request fails", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "atomic", profiles: ["atomic"] };

    const routeMock = globalThis.fetch as unknown as FetchMock;
    let settlePost: (response: Response) => void = () => undefined;
    const pendingPost = new Promise<Response>((resolve) => {
      settlePost = resolve;
    });

    try {
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          if ((init?.method ?? "GET") !== "POST") return routeMock(input, init);
          return pendingPost;
        }),
      );

      let open = true;
      const noop = vi.fn();
      const view = render(<TrainingModal open={open} onOpenChange={noop} />);

      await screen.findByTestId("training-state");

      fireEvent.click(screen.getAllByRole("button", { name: "Unsloth train" })[0]);
      fireEvent.click(await screen.findByRole("button", { name: "Confirm" }));
      open = false;
      view.rerender(<TrainingModal open={open} onOpenChange={noop} />);

      await act(async () => {
        settlePost(
          new Response(JSON.stringify({ ok: false, error: "boom" }), {
            status: 500,
            headers: { "content-type": "application/json" },
          }),
        );
      });

      open = true;
      view.rerender(<TrainingModal open={open} onOpenChange={noop} />);
      await screen.findByTestId("training-state");
      expect(screen.queryByTestId("training-action-error")).toBeNull();
    } finally {
      vi.stubGlobal("fetch", routeMock);
    }
  });

  it("does not refetch status when the dialog closes while an in-flight action completes", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "atomic", profiles: ["atomic"] };

    const routeMock = globalThis.fetch as unknown as FetchMock;
    let settlePost: (response: Response) => void = () => undefined;
    const pendingPost = new Promise<Response>((resolve) => {
      settlePost = resolve;
    });

    try {
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          if ((init?.method ?? "GET") !== "POST") return routeMock(input, init);
          return pendingPost;
        }),
      );

      let open = true;
      const noop = vi.fn();
      const view = render(<TrainingModal open={open} onOpenChange={noop} />);
      await screen.findByTestId("training-state");

      fireEvent.click(screen.getAllByRole("button", { name: "Unsloth train" })[0]);
      fireEvent.click(await screen.findByRole("button", { name: "Confirm" }));
      open = false;
      view.rerender(<TrainingModal open={open} onOpenChange={noop} />);

const calls = (routeMock as unknown as {
        mock: { calls: Array<[RequestInfo | URL, RequestInit?]> };
      }).mock.calls;
      const statusCalls = () => calls.filter((call) => String(call[0]).includes("/status")).length;
      const countBefore = statusCalls();

      await act(async () => {
        settlePost(jsonResponse({ ok: true }));
      });
      expect(statusCalls()).toBe(countBefore);

      open = true;
      view.rerender(<TrainingModal open={open} onOpenChange={noop} />);
      await screen.findByTestId("training-state");
      expect(statusCalls()).toBeGreaterThan(countBefore);
    } finally {
      vi.stubGlobal("fetch", routeMock);
    }
  });

  it("loads profiles from the single status fetch and never requests the models endpoint", async () => {
    const fetchMock = vi.mocked(globalThis.fetch);
    fetchMock.mockClear();
    statusRoute.body = { ...idleStatus, active_profile: "atomic", runtime_up: true };
    openModal();
    await screen.findByTestId("training-state");

    const urls = fetchMock.mock.calls.map((call) => String(call[0]));
    expect(urls.some((url) => url.includes("/api/training/status"))).toBe(true);
    expect(urls.some((url) => url.includes("/api/training/models"))).toBe(false);
  });

  it("shows a compact error banner for host-unreachable responses", async () => {
    statusRoute.body = { ok: false, error: "host unreachable" };
    statusRoute.status = 502;
    openModal();
    expect(await screen.findByTestId("training-status-error")).toHaveTextContent(
      "host unreachable",
    );
  });

  it("shows the status fetch error in the model list banner after a later poll fails, then clears it", async () => {
    openModal();
    await screen.findByTestId("training-state");
    expect(screen.getByRole("heading", { name: "llama.cpp & profiles" })).toBeTruthy();
    expect(screen.queryByTestId("training-profiles-error")).toBeNull();

    statusRoute.body = { ok: false, error: "training host command failed" };
    statusRoute.status = 502;
    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    const banner = await screen.findByTestId("training-profiles-error");
    expect(banner.textContent?.startsWith("Model list unavailable:")).toBe(true);
    expect(banner.textContent).toContain("training host command failed");
    expect(banner.textContent).not.toContain("<");
    expect(screen.queryByTestId("training-status-error")).toBeNull();

    statusRoute.body = idleStatus;
    statusRoute.status = undefined;
    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    await waitFor(() => expect(screen.queryByTestId("training-profiles-error")).toBeNull());
  });

  it("calls stopTraining once when Confirm is double-clicked", async () => {
    let resolveStop: () => void = () => undefined;
    const stopTraining = vi.spyOn(trainingApi, "stopTraining").mockReturnValue(
      new Promise<void>((resolve) => {
        resolveStop = resolve;
      }),
    );
    statusRoute.body = {
      ...idleStatus,
      run: {
        kind: "lfm-train",
        started: "2026-07-19T08:00:00Z",
        running: true,
        progress: null,
        log_tail: [],
      },
    };
    try {
      openModal();
      fireEvent.click(await screen.findByRole("button", { name: "Stop run" }));
      const confirm = await screen.findByRole("button", { name: "Confirm" });
      act(() => {
        confirm.click();
        confirm.click();
      });
      expect(stopTraining).toHaveBeenCalledTimes(1);
      expect(confirm).toBeDisabled();
      expect(confirm).toHaveTextContent("Stopping run…");
      expect(confirm).toHaveAttribute("aria-busy", "true");
    } finally {
      stopTraining.mockRestore();
      await act(async () => resolveStop());
    }
  });

  it("starts a double-clicked job once and re-enables Confirm after it settles", async () => {
    let resolveStart: () => void = () => undefined;
    const startTrainingJob = vi.spyOn(trainingApi, "startTrainingJob").mockReturnValue(
      new Promise<void>((resolve) => {
        resolveStart = resolve;
      }),
    );
    try {
      openModal();
      fireEvent.click((await screen.findAllByRole("button", { name: "Unsloth train" })).at(-1)!);
      const confirm = await screen.findByRole("button", { name: "Confirm" });
      act(() => {
        confirm.click();
        confirm.click();
      });
      expect(startTrainingJob).toHaveBeenCalledTimes(1);
      expect(confirm).toBeDisabled();

      await act(async () => resolveStart());
      await waitFor(() => expect(screen.queryByTestId("training-confirm")).toBeNull());

      fireEvent.click((await screen.findAllByRole("button", { name: "Unsloth train" })).at(-1)!);
      const nextConfirm = await screen.findByRole("button", { name: "Confirm" });
      expect(nextConfirm).toBeEnabled();
      fireEvent.click(nextConfirm);
      await waitFor(() => expect(startTrainingJob).toHaveBeenCalledTimes(2));
    } finally {
      startTrainingJob.mockRestore();
    }
  });

  it("shows an action error banner when a confirmed request fails with 409", async () => {
    statusRoute.body = {
      ...idleStatus,
      run: { kind: "lfm-train", started: "", running: true, progress: null, log_tail: [] },
    };
    openModal();
    fireEvent.click(await screen.findByRole("button", { name: "Stop run" }));

    expect(await screen.findByText("Stop the current training run?")).toBeTruthy();
    postResponse = { body: { ok: false, error: "a job is running" }, status: 409 };
    fireEvent.click(await screen.findByRole("button", { name: "Confirm" }));
    expect(await screen.findByTestId("training-action-error")).toHaveTextContent(
      "a job is running",
    );
  });

  it("exposes TrainingModal through public.ts only", async () => {
    const mod = await import("./public");
    expect(Object.keys(mod)).toEqual(["TrainingModal"]);
  });
});
