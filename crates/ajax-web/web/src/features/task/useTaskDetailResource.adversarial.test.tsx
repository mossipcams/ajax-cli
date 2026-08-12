// Opt-in. AJAX_CHAOS=1 npm run web:test -- --run src/features/task/useTaskDetailResource.adversarial.test.tsx

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import taskDetailFixture from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { useTaskDetailResource } from "./useTaskDetailResource";

const chaos = process.env.AJAX_CHAOS === "1";
const taskDetail = taskDetailFixture as BrowserTaskDetail;
const staleDetail: BrowserTaskDetail = { ...taskDetail, title: "STALE TITLE" };
const freshDetail: BrowserTaskDetail = { ...taskDetail, title: "FRESH TITLE" };

const fetchDetail = vi.fn<(handle: string) => Promise<BrowserTaskDetail>>();
const postOperation = vi.fn();

vi.mock("@/shared/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/shared/lib/api")>();
  return {
    ...actual,
    fetchDetail: (handle: string) => fetchDetail(handle),
    postOperation: (...args: Parameters<typeof actual.postOperation>) => postOperation(...args),
    requestId: () => "test-request-id",
  };
});

describe.runIf(chaos)("useTaskDetailResource adversarial", () => {
  beforeEach(() => {
    fetchDetail.mockReset();
    postOperation.mockReset();
    postOperation.mockResolvedValue({ ok: false, response: {} });
  });
  afterEach(() => vi.clearAllMocks());

  it("does not let a slower same-handle fetch overwrite a newer one", async () => {
    let resolveSlow!: (value: BrowserTaskDetail) => void;
    let resolveFast!: (value: BrowserTaskDetail) => void;
    const slow = new Promise<BrowserTaskDetail>((res) => {
      resolveSlow = res;
    });
    const fast = new Promise<BrowserTaskDetail>((res) => {
      resolveFast = res;
    });
    fetchDetail.mockReturnValueOnce(slow).mockReturnValueOnce(fast);

    const deps = {
      applyCockpit: vi.fn(),
      applyConnectionError: vi.fn(),
      markConnected: vi.fn(),
    };
    const { result } = renderHook(() => useTaskDetailResource("web/fix-login", deps));

    // First load in flight (slow). Trigger reload (fast).
    await act(async () => {
      result.current.reload();
    });

    await act(async () => {
      resolveFast(freshDetail);
      await new Promise((r) => setTimeout(r, 0));
    });
    await waitFor(() => expect(result.current.detail.data?.title).toBe("FRESH TITLE"));

    await act(async () => {
      resolveSlow(staleDetail);
      await new Promise((r) => setTimeout(r, 0));
    });

    expect(result.current.detail.data?.title).toBe("FRESH TITLE");
  });

  it("posts resume at most once per handle open under StrictMode-like double effect", async () => {
    fetchDetail.mockResolvedValue(taskDetail);
    postOperation.mockResolvedValue({ ok: true, response: {} });
    const deps = {
      applyCockpit: vi.fn(),
      applyConnectionError: vi.fn(),
      markConnected: vi.fn(),
    };
    const { rerender } = renderHook(
      ({ handle }) => useTaskDetailResource(handle, deps),
      { initialProps: { handle: "web/fix-login" as string | null } },
    );
    // Remount-equivalent: clear handle then set again quickly.
    rerender({ handle: null });
    rerender({ handle: "web/fix-login" });
    rerender({ handle: "web/fix-login" });
    await waitFor(() => expect(fetchDetail).toHaveBeenCalled());
    await waitFor(() => expect(postOperation.mock.calls.length).toBeGreaterThan(0));
    const resumes = postOperation.mock.calls.filter(
      (call) => (call[0] as { action?: string }).action === "resume",
    );
    expect(resumes.length).toBeLessThanOrEqual(1);
  });
});

describe("detail adversarial gate", () => {
  it("documents AJAX_CHAOS opt-in", () => {
    expect(typeof chaos).toBe("boolean");
  });
});
