// Round 8 — HIGH-severity hunts only (wrong-task side effects, stuck UI, truth clobber).
// Skip latch nits. AJAX_CHAOS=1 npm run web:test -- --run src/features/task/useTaskDetailResource.severe.test.tsx

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import taskDetailFixture from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { useTaskDetailResource } from "./useTaskDetailResource";
import { ApiError } from "@/shared/lib/api";

const chaos = process.env.AJAX_CHAOS === "1";
const taskDetail = taskDetailFixture as BrowserTaskDetail;

const fetchDetail = vi.fn<(handle: string) => Promise<BrowserTaskDetail>>();
const postOperation = vi.fn();

vi.mock("@/shared/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/shared/lib/api")>();
  return {
    ...actual,
    fetchDetail: (handle: string) => fetchDetail(handle),
    postOperation: (...args: Parameters<typeof actual.postOperation>) => postOperation(...args),
    requestId: () => "severe-request-id",
  };
});

describe.runIf(chaos)("useTaskDetailResource SEVERE", () => {
  beforeEach(() => {
    fetchDetail.mockReset();
    postOperation.mockReset();
  });
  afterEach(() => vi.clearAllMocks());

  it("HIGH: leaving a task cancels in-flight resumeOnOpen (no side-effect resume on abandoned task)", async () => {
    let resolveResume!: (value: unknown) => void;
    const resumePending = new Promise((resolve) => {
      resolveResume = resolve;
    });
    fetchDetail.mockResolvedValue(taskDetail);
    postOperation.mockReturnValue(resumePending as never);

    const applyCockpit = vi.fn();
    const { rerender } = renderHook(
      ({ handle }) =>
        useTaskDetailResource(handle, {
          applyCockpit,
          applyConnectionError: vi.fn(),
          markConnected: vi.fn(),
        }),
      { initialProps: { handle: "ajax-cli/grdt" as string | null } },
    );

    await waitFor(() => expect(postOperation).toHaveBeenCalled());
    expect(postOperation.mock.calls[0]?.[0]).toMatchObject({
      action: "resume",
      task_handle: "ajax-cli/grdt",
    });

    // Operator leaves before resume returns.
    rerender({ handle: null });

    await act(async () => {
      resolveResume({
        ok: true,
        response: {
          cockpit: {
            cards: [{ qualified_handle: "ajax-cli/grdt", title: "SHOULD NOT APPLY" }],
          },
        },
      });
      await new Promise((r) => setTimeout(r, 0));
    });

    // Product expectation: abandoning the task must not apply its resume cockpit
    // and ideally should not leave a fire-and-forget resume mutation outstanding
    // without cancellation. At minimum, cockpit from the abandoned resume must
    // not clobber dashboard truth.
    expect(applyCockpit).not.toHaveBeenCalled();
  });

  it("HIGH: network failure on first detail load must surface TaskLoadError path (not eternal skeleton)", async () => {
    fetchDetail.mockRejectedValue(new ApiError("network", "Failed to fetch"));
    postOperation.mockResolvedValue({ ok: false, response: {} });

    const { result } = renderHook(() =>
      useTaskDetailResource("ajax-cli/grdt", {
        applyCockpit: vi.fn(),
        applyConnectionError: vi.fn(),
        markConnected: vi.fn(),
      }),
    );

    await waitFor(() => expect(fetchDetail).toHaveBeenCalled());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });

    // Eternal loading with null data = stuck skeleton with no Retry (#797 class).
    expect(result.current.detail.status).not.toBe("loading");
    expect(result.current.detail.status).toBe("error");
  });

  it("HIGH: switching A→B must not show A's detail under B's route handle", async () => {
    let resolveA!: (value: BrowserTaskDetail) => void;
    const pendingA = new Promise<BrowserTaskDetail>((resolve) => {
      resolveA = resolve;
    });
    fetchDetail.mockImplementation((handle: string) => {
      if (handle === "ajax-cli/a") return pendingA;
      return Promise.resolve({ ...taskDetail, qualified_handle: "ajax-cli/b", title: "Task B" });
    });
    postOperation.mockResolvedValue({ ok: false, response: {} });

    const { result, rerender } = renderHook(
      ({ handle }) =>
        useTaskDetailResource(handle, {
          applyCockpit: vi.fn(),
          applyConnectionError: vi.fn(),
          markConnected: vi.fn(),
        }),
      { initialProps: { handle: "ajax-cli/a" as string | null } },
    );

    rerender({ handle: "ajax-cli/b" });
    await waitFor(() => expect(result.current.detail.data?.title).toBe("Task B"));

    await act(async () => {
      resolveA({ ...taskDetail, qualified_handle: "ajax-cli/a", title: "Task A LATE" });
      await new Promise((r) => setTimeout(r, 0));
    });

    expect(result.current.detail.data?.qualified_handle).toBe("ajax-cli/b");
    expect(result.current.detail.data?.title).toBe("Task B");
  });
});
