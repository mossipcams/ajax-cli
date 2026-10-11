import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, fireEvent, screen, waitFor } from "@/test/testing-library-shim";
import { render } from "@/test/testing-library-shim";
import TrainingModal from "./TrainingModal";

const statusRoute: { body: unknown; status?: number } = { body: {} };
const modelsRoute: { body: unknown; status?: number } = { body: {} };
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
    if (url.includes("/api/training/models")) {
      return jsonResponse(modelsRoute.body, modelsRoute.status);
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
  runtime_up: true,
  active_profile: null,
  run: null,
  generation: null,
  profile_details: {
    atomic: { label: "Atomic", model: "qwen3-8b", serving: "ctx 122880, 2 slots" },
    "swift-1.5": { label: "Swift 1.5", model: "qwen3-4b", serving: "" },
  },
};

const defaultModels = {
  ok: true,
  profiles: ["atomic", "swift-1.5"],
  active_profile: null,
  running: false,
};

const onOpenChange = vi.fn();
function openModal(): void {
  render(<TrainingModal open onOpenChange={onOpenChange} />);
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  postCalls.length = 0;
  statusRoute.body = idleStatus;
  statusRoute.status = undefined;
  modelsRoute.body = defaultModels;
  modelsRoute.status = undefined;
  postResponse = { body: { ok: true } };
});

afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
});

describe("TrainingModal", () => {
  it("renders idle state with no run and no generation activity", async () => {
    openModal();
    expect(await screen.findByTestId("training-state")).toHaveTextContent("Idle");
    // The raw host state remains visible, verbatim, under the human line.
    expect(screen.getByTestId("training-raw-state")).toHaveTextContent("idle");
    expect(screen.getByText("No run in progress.")).toBeTruthy();
    expect(screen.getByText("No generation activity.")).toBeTruthy();
  });

  it("renders the serving profile with a stop action", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "atomic" };
    modelsRoute.body = {
      ok: true,
      profiles: ["atomic", "swift-1.5"],
      active_profile: "atomic",
      running: true,
    };
    openModal();
    expect(await screen.findByTestId("training-state")).toBeTruthy();
    expect(screen.getByText("serving")).toBeTruthy();
    expect(screen.getByText("ctx 122880, 2 slots")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Stop" })).toBeTruthy();
  });

  it("renders a training run with progress, loss, ETA and log tail", async () => {
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
    // Headline is human; the raw host state stays verbatim below it.
    expect(await screen.findByTestId("training-state")).toHaveTextContent(
      "Unsloth train",
    );
    expect(screen.getByTestId("training-raw-state")).toHaveTextContent(
      "train:lfm-train",
    );
    const run = screen.getByTestId("training-run");
    expect(run).toHaveTextContent(/lfm-train/);
    expect(screen.getByRole("progressbar", { name: "lfm-train progress" })).toBeTruthy();
    expect(run).toHaveTextContent(/step 30 \/ 100/);
    expect(run).toHaveTextContent(/loss 1\.2345/);
    expect(run).toHaveTextContent(/ETA ~6 min/);
    const log = screen.getByTestId("training-log");
    expect(Array.from(log.querySelectorAll("li")).map((el) => el.textContent)).toEqual([
      "loss=1.2",
      "step 30",
    ]);
  });

  it("renders generation with target and rows-only when target is null", async () => {
    statusRoute.body = { ...idleStatus, state: "generate", generation: { rows: 40, target: 100, running: true } };
    openModal();
    expect(await screen.findByText(/40 \/ 100 rows/)).toBeTruthy();

    statusRoute.body = { ...idleStatus, state: "generate", generation: { rows: 7, target: null, running: false } };
    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    expect(await screen.findByTestId("training-generation-rows")).toHaveTextContent("7 rows");
  });

  it("shows a live training job from the host state when no run object is sent", async () => {
    statusRoute.body = { ...idleStatus, state: "train:lfm 123 456 1700000000", run: null };
    openModal();
    expect(await screen.findByTestId("training-live-state")).toHaveTextContent("Training in progress: lfm");
    expect(screen.queryByText("No run in progress.")).toBeNull();
  });

  it("sends no mutating request without an explicit confirm step", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "llama" };
    modelsRoute.body = { ok: true, profiles: ["llama", "unsloth"], active_profile: "llama", running: false };
    openModal();
    // Profile switch is two-step: arming must not fire a mutating request.
    fireEvent.click(await screen.findByRole("button", { name: /unsloth/ }));
    expect(postCalls).toHaveLength(0);

    fireEvent.click(
      await screen.findByRole("button", { name: "Confirm switch to unsloth" }),
    );
    await waitFor(() => expect(postCalls).toHaveLength(1));
    expect(postCalls[0].url).toContain("/api/training/models/switch");
    expect(postCalls[0].body).toEqual({ profile: "unsloth", confirm: true });

    fireEvent.click(await screen.findByRole("button", { name: "Unsloth train" }));
    expect(postCalls).toHaveLength(1);
    fireEvent.click(await screen.findByRole("button", { name: "Confirm Unsloth train" }));
    await waitFor(() => expect(postCalls).toHaveLength(2));
    expect(postCalls[1].url).toContain("/api/training/start");
    expect(postCalls[1].body).toEqual({ job: "lfm-train", confirm: true });
  });

  it("disables profile switching while busy and says why", async () => {
    statusRoute.body = { ...idleStatus, state: "train:lfm-train" };
    modelsRoute.body = { ok: true, profiles: ["llama", "unsloth"], active_profile: "llama", running: false };
    openModal();
    expect(await screen.findByRole("button", { name: /unsloth/ })).toBeDisabled();
    expect(
      screen.getByText(/Switching is unavailable while (a training job is in progress|state is train:)/),
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
    // Delegate GETs to the existing per-route mock; let POSTs hang so the start
    // request stays in flight while we assert.
    const routeMock = globalThis.fetch as (
      i: RequestInfo | URL,
      o?: RequestInit,
    ) => Promise<Response>;

    // A stopped runtime with a selected profile makes Start clickable.
    statusRoute.body = { ...idleStatus, active_profile: "atomic" };
    modelsRoute.body = { ok: true, profiles: ["atomic", "swift-1.5"], active_profile: "atomic", running: false };
    const hang = new Promise<Response>(() => undefined);
    try {
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          if ((init?.method ?? "GET") !== "POST") return routeMock(input, init);
          return hang;
        }),
      );
      openModal();

      await screen.findByTestId("training-state");

      fireEvent.click(screen.getByRole("button", { name: "Start" }));

      const busyButton = await screen.findByRole("button", { name: /Starting…/ });
      expect(busyButton).toHaveAttribute("aria-busy", "true");
    } finally {
      // Restore the per-route mock so POSTs don't hang for later tests.
      vi.stubGlobal("fetch", routeMock);
    }
  });

  it("cancelling an armed profile switch sends no request", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "llama" };
    modelsRoute.body = { ok: true, profiles: ["llama", "unsloth"], active_profile: "llama", running: false };
    openModal();

    fireEvent.click(await screen.findByRole("button", { name: /unsloth/ }));
    const cancel = await screen.findByRole("button", { name: "Cancel" });
    fireEvent.click(cancel);

    // No request, and the armed UI is gone — picker is back in its default state.
    expect(postCalls).toHaveLength(0);
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull();
    expect(await screen.findByRole("button", { name: /unsloth/ })).toBeTruthy();
  });

  it("shows a compact error banner for host-unreachable responses", async () => {
    statusRoute.body = { ok: false, error: "host unreachable" };
    statusRoute.status = 502;
    openModal();
    expect(await screen.findByTestId("training-status-error")).toHaveTextContent(
      "host unreachable",
    );
  });

  it("shows an action error banner when a confirmed request fails with 409", async () => {
    statusRoute.body = {
      ...idleStatus,
      run: { kind: "lfm-train", started: "", running: true, progress: null, log_tail: [] },
    };
    openModal();
    fireEvent.click(await screen.findByRole("button", { name: "Stop run" }));

    postResponse = { body: { ok: false, error: "a job is running" }, status: 409 };
    fireEvent.click(await screen.findByRole("button", { name: "Confirm stop run" }));
    expect(await screen.findByTestId("training-action-error")).toHaveTextContent(
      "a job is running",
    );
  });

  it("exposes TrainingModal through public.ts only", async () => {
    const mod = await import("./public");
    expect(Object.keys(mod)).toEqual(["TrainingModal"]);
  });
});
