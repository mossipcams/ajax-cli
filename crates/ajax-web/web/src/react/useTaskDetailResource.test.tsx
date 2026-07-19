import { describe, it, expect, afterEach, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import taskDetailFixture from "../fixtures/task-detail.json";
import type { BrowserCockpitView, BrowserTaskDetail } from "../types";
import { ApiError } from "../api";
import { useTaskDetailResource } from "./useTaskDetailResource";

const taskDetail = taskDetailFixture as BrowserTaskDetail;
const otherDetail: BrowserTaskDetail = {
  ...taskDetail,
  qualified_handle: "web/other",
  title: "Other task",
};

const fetchDetail = vi.fn<(handle: string) => Promise<BrowserTaskDetail>>();
const postOperation = vi.fn();

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return {
    ...actual,
    fetchDetail: (handle: string) => fetchDetail(handle),
    postOperation: (...args: Parameters<typeof actual.postOperation>) => postOperation(...args),
    requestId: () => "test-request-id",
  };
});

function stableDeps() {
  const applyCockpit = vi.fn<(next: BrowserCockpitView) => void>();
  const applyConnectionError = vi.fn<(error: unknown) => void>();
  const markConnected = vi.fn();
  return { applyCockpit, applyConnectionError, markConnected };
}

describe("useTaskDetailResource", () => {
  beforeEach(() => {
    fetchDetail.mockReset();
    postOperation.mockReset();
    postOperation.mockResolvedValue({ ok: true, response: {} });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("maps no handle to loading", () => {
    const deps = stableDeps();
    const { result } = renderHook(() => useTaskDetailResource(null, deps));
    expect(result.current.detail).toEqual({
      status: "loading",
      data: null,
      error: null,
    });
  });

  it("maps handle set with first load in flight to loading", () => {
    let resolve!: (value: BrowserTaskDetail) => void;
    fetchDetail.mockReturnValue(
      new Promise<BrowserTaskDetail>((res) => {
        resolve = res;
      }),
    );
    const deps = stableDeps();
    const { result } = renderHook(() => useTaskDetailResource("web/fix-login", deps));

    expect(result.current.detail).toEqual({
      status: "loading",
      data: null,
      error: null,
    });

    act(() => {
      resolve(taskDetail);
    });
  });

  it("maps a successful load to ready", async () => {
    fetchDetail.mockResolvedValue(taskDetail);
    const deps = stableDeps();
    const { result } = renderHook(() => useTaskDetailResource("web/fix-login", deps));

    await waitFor(() => expect(result.current.detail.status).toBe("ready"));
    expect(result.current.detail).toEqual({
      status: "ready",
      data: taskDetail,
      error: null,
    });
    expect(deps.markConnected).toHaveBeenCalled();
  });

  it("maps first load failure with no prior detail to error", async () => {
    fetchDetail.mockRejectedValue(new ApiError("http", "HTTP 503", 503));
    const deps = stableDeps();
    const { result } = renderHook(() => useTaskDetailResource("web/fix-login", deps));

    await waitFor(() => expect(result.current.detail.status).toBe("error"));
    expect(result.current.detail.data).toBeNull();
    expect(result.current.detail.error).toMatchObject({ kind: "http", message: "HTTP 503" });
    expect(deps.applyConnectionError).toHaveBeenCalled();
  });

  it("maps load failure with existing detail for this handle to stale", async () => {
    postOperation.mockResolvedValue({ ok: false, response: {} });
    fetchDetail.mockResolvedValueOnce(taskDetail).mockRejectedValueOnce(
      new ApiError("http", "HTTP 503", 503),
    );
    const deps = stableDeps();
    const { result } = renderHook(() => useTaskDetailResource("web/fix-login", deps));

    await waitFor(() => expect(result.current.detail.status).toBe("ready"));

    await act(async () => {
      await result.current.reload();
    });

    await waitFor(() => expect(result.current.detail.status).toBe("stale"));
    expect(result.current.detail.data).toEqual(taskDetail);
    expect(result.current.detail.error).toMatchObject({ kind: "http", message: "HTTP 503" });
  });

  it("discards a slow response for handle A after switching to handle B", async () => {
    let resolveFirst!: (value: BrowserTaskDetail) => void;
    const firstPending = new Promise<BrowserTaskDetail>((res) => {
      resolveFirst = res;
    });
    fetchDetail.mockImplementation((handle: string) => {
      if (handle === "web/fix-login") return firstPending;
      if (handle === "web/other") return Promise.resolve(otherDetail);
      return Promise.reject(new Error(`unexpected handle: ${handle}`));
    });
    const deps = stableDeps();
    const { result, rerender } = renderHook(
      ({ handle }) => useTaskDetailResource(handle, deps),
      { initialProps: { handle: "web/fix-login" as string | null } },
    );

    rerender({ handle: "web/other" });
    await waitFor(() => expect(result.current.detail.data?.title).toBe("Other task"));

    await act(async () => {
      resolveFirst({ ...taskDetail, title: "STALE fix-login" });
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(result.current.detail.data?.title).toBe("Other task");
    expect(result.current.detail.status).toBe("ready");
  });

  it("resets to loading when switching handles and never shows A data under B", async () => {
    let resolveFirst!: (value: BrowserTaskDetail) => void;
    const firstPending = new Promise<BrowserTaskDetail>((res) => {
      resolveFirst = res;
    });
    fetchDetail.mockImplementation((handle: string) => {
      if (handle === "web/fix-login") return firstPending;
      if (handle === "web/other") return Promise.resolve(otherDetail);
      return Promise.reject(new Error(`unexpected handle: ${handle}`));
    });
    const deps = stableDeps();
    const { result, rerender } = renderHook(
      ({ handle }) => useTaskDetailResource(handle, deps),
      { initialProps: { handle: "web/fix-login" as string | null } },
    );

    expect(result.current.detail.status).toBe("loading");
    expect(result.current.detail.data).toBeNull();

    rerender({ handle: "web/other" });
    expect(result.current.detail.status).toBe("loading");
    expect(result.current.detail.data).toBeNull();

    await waitFor(() => expect(result.current.detail.data?.title).toBe("Other task"));

    await act(async () => {
      resolveFirst(taskDetail);
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(result.current.detail.data?.title).toBe("Other task");
  });

  it("reload refetches the current handle", async () => {
    postOperation.mockResolvedValue({ ok: false, response: {} });
    fetchDetail.mockResolvedValue(taskDetail);
    const deps = stableDeps();
    const { result } = renderHook(() => useTaskDetailResource("web/fix-login", deps));

    await waitFor(() => expect(result.current.detail.status).toBe("ready"));
    expect(fetchDetail).toHaveBeenCalledTimes(1);

    await act(async () => {
      await result.current.reload();
    });

    expect(fetchDetail).toHaveBeenCalledTimes(2);
    expect(fetchDetail).toHaveBeenLastCalledWith("web/fix-login");
  });

  it("keeps reload referentially stable across re-renders", async () => {
    fetchDetail.mockResolvedValue(taskDetail);
    const deps = stableDeps();
    const { result, rerender } = renderHook(
      ({ handle }) => useTaskDetailResource(handle, deps),
      { initialProps: { handle: "web/fix-login" as string | null } },
    );

    await waitFor(() => expect(result.current.detail.status).toBe("ready"));
    const firstReload = result.current.reload;
    rerender({ handle: "web/fix-login" });
    rerender({ handle: "web/fix-login" });
    expect(result.current.reload).toBe(firstReload);
  });
});
