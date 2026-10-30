import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@/test/testing-library-shim";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import TrainingModal from "./TrainingModal";

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
  it("shows a skeleton while the first status fetch is pending, then the model list", async () => {
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
      expect(screen.queryByRole("button", { name: "Start serving" })).toBeNull();

      await act(async () => {
        settleStatus(jsonResponse(idleStatus));
        vi.advanceTimersByTime(10);
      });
      expect(await screen.findByRole("heading", { name: "llama.cpp & profiles" })).toBeTruthy();
      expect(screen.queryByTestId("training-loading")).toBeNull();
    } finally {
      vi.stubGlobal("fetch", routeMock);
    }
  });

  it("names the dialog and its heading Local", async () => {
    openModal();
    await screen.findByRole("heading", { name: "llama.cpp & profiles" });
    expect(screen.getByRole("dialog", { name: "Local" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Local", level: 2 })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Training", level: 2 })).toBeNull();
  });

  it("renders the serving profile with a stop action", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "atomic", runtime_up: true };
    openModal();
    expect(await screen.findByTestId("llama-state")).toHaveTextContent("Serving · atomic");
    expect(screen.getByText("serving")).toBeTruthy();
    expect(screen.getByText("ctx 122880, 2 slots")).toBeTruthy();
    expect(
      screen.getAllByRole("button", { name: "Stop serving" }).length,
    ).toBeGreaterThan(0);
  });

  it("shows only the run label, elapsed time, and step progress while a run is active", async () => {
    statusRoute.body = {
      ...idleStatus,
      state: "train:lfm-train",
      active_profile: "unsloth",
      run: {
        kind: "lfm-train",
        started: "2026-07-19T08:00:00Z",
        running: true,
        progress: { step: 1240, total: 3000, loss: 1.2345, eta_s: 360 },
        log_tail: ["loss=1.2", "step 1240"],
      },
    };
    openModal();
    const block = await screen.findByTestId("training-local-run");
    expect(screen.getByRole("region", { name: "Training progress" })).toBe(block);
    expect(block).toHaveTextContent("Unsloth train");
    expect(block).toHaveTextContent(/elapsed/);
    expect(block).toHaveTextContent("Step 1240 / 3000");
    expect(block).not.toHaveTextContent("1.23");
    expect(block).not.toHaveTextContent("~6 min");

    const bar = screen.getByRole("progressbar", { name: "Training step progress" });
    expect(bar).toHaveAttribute("max", "3000");
    expect(bar).toHaveAttribute("value", "1240");

    for (const name of ["Stop run", "Unsloth train", "Data generation", "Wake train"]) {
      expect(screen.queryByRole("button", { name })).toBeNull();
    }
  });

  it("renders no training block when no run is active", async () => {
    openModal();
    await screen.findByRole("heading", { name: "llama.cpp & profiles" });
    expect(screen.queryByTestId("training-local-run")).toBeNull();
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

    fireEvent.click(screen.getByRole("button", { name: "Start serving" }));
    expect(postCalls).toHaveLength(1);
    const serveConfirm = await screen.findByTestId("training-confirm");
    expect(serveConfirm).toHaveTextContent("Start llama.cpp serving for the active profile?");

    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(postCalls).toHaveLength(2));
    expect(postCalls[1].url).toContain("/api/training/serve");
    expect(postCalls[1].body).toEqual({ confirm: true });
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

  it("disables profile switching while a serve request is in flight", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "atomic" };
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
      openModal();
      fireEvent.click(await screen.findByRole("button", { name: "Start serving" }));
      fireEvent.click(await screen.findByRole("button", { name: "Confirm" }));

      expect(await screen.findByRole("button", { name: /Swift 1\.5/ })).toBeDisabled();
      expect(screen.getByRole("button", { name: /Atomic/ })).toBeDisabled();
      expect(screen.queryByText(/Switching is unavailable while/)).toBeNull();

      await act(async () => {
        settlePost(jsonResponse({ ok: true }));
      });
    } finally {
      vi.stubGlobal("fetch", routeMock);
    }
  });

  it("polls status while open and stops polling when closed", async () => {
    const fetchMock = vi.mocked(globalThis.fetch);
    let open = true;
    const { rerender } = render(<TrainingModal open={open} onOpenChange={onOpenChange} />);
    await screen.findByRole("heading", { name: "llama.cpp & profiles" });

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
    statusRoute.body = { ...idleStatus, active_profile: "atomic" };

    openModal();
    await screen.findByRole("heading", { name: "llama.cpp & profiles" });

    fireEvent.click(screen.getByRole("button", { name: "Start serving" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirm" }));

    try {
      const busyButton = await screen.findByRole("button", { name: /Starting/ });
      expect(busyButton).toHaveAttribute("aria-busy", "true");
    } finally {
      vi.stubGlobal("fetch", routeMock);
    }
  });

  it("sends serve once when Confirm is double-clicked and re-enables Confirm after it settles", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "atomic" };
    const routeMock = globalThis.fetch as unknown as FetchMock;
    let settlePost: (response: Response) => void = () => undefined;
    const pendingPost = new Promise<Response>((resolve) => {
      settlePost = resolve;
    });
    const servePosts = () =>
      vi.mocked(globalThis.fetch).mock.calls.filter(
        ([url, init]) => String(url).includes("/api/training/serve") && init?.method === "POST",
      ).length;

    try {
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
          if ((init?.method ?? "GET") !== "POST") return routeMock(input, init);
          return pendingPost;
        }),
      );
      openModal();
      fireEvent.click(await screen.findByRole("button", { name: "Start serving" }));
      const confirm = await screen.findByRole("button", { name: "Confirm" });
      fireEvent.click(confirm);
      fireEvent.click(confirm);

      expect(servePosts()).toBe(1);
      expect(confirm).toBeDisabled();

      await act(async () => {
        settlePost(
          new Response(JSON.stringify({ ok: false, error: "boom" }), {
            status: 500,
            headers: { "content-type": "application/json" },
          }),
        );
      });
      expect(await screen.findByTestId("training-action-error")).toHaveTextContent("boom");
      expect(screen.getByRole("button", { name: "Confirm" })).toBeEnabled();
      expect(servePosts()).toBe(1);
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

      await screen.findByRole("heading", { name: "llama.cpp & profiles" });

      fireEvent.click(screen.getByRole("button", { name: "Start serving" }));
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
      await screen.findByRole("heading", { name: "llama.cpp & profiles" });
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
      await screen.findByRole("heading", { name: "llama.cpp & profiles" });

      fireEvent.click(screen.getByRole("button", { name: "Start serving" }));
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
      await screen.findByRole("heading", { name: "llama.cpp & profiles" });
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
    await screen.findByRole("heading", { name: "llama.cpp & profiles" });

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
    await screen.findByRole("heading", { name: "llama.cpp & profiles" });
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

  it("shows an action error banner when a confirmed request fails with 409", async () => {
    statusRoute.body = { ...idleStatus, active_profile: "atomic" };
    openModal();
    fireEvent.click(await screen.findByRole("button", { name: "Start serving" }));

    expect(
      await screen.findByText("Start llama.cpp serving for the active profile?"),
    ).toBeTruthy();
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
