import { useCallback, useEffect, useMemo, useRef } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ApiError, fetchDetail, requestId } from "@/shared/lib/api";
import { queryKeys } from "@/shared/lib/queryClient";
import type { BrowserCockpitView, BrowserTaskDetail, RemoteResource } from "@/shared/lib/types";
import { useTaskOperationMutation } from "./useTaskOperationMutation";

export type TaskDetailResourceDeps = {
  applyCockpit: (next: BrowserCockpitView) => void;
  applyConnectionError: (error: unknown) => void;
  markConnected: () => void;
};

function toDetailError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  return new ApiError("incompatible", error instanceof Error ? error.message : String(error));
}

export function useTaskDetailResource(
  handle: string | null,
  deps: TaskDetailResourceDeps,
): {
  detail: RemoteResource<BrowserTaskDetail>;
  reload: () => void;
} {
  const depsRef = useRef(deps);
  depsRef.current = deps;

  const handleRef = useRef(handle);
  handleRef.current = handle;

  const resumedHandleRef = useRef<string | null>(null);
  const queryClient = useQueryClient();
  const executeOperation = useTaskOperationMutation();

  const query = useQuery({
    queryKey: handle ? queryKeys.taskDetail(handle) : ["task-detail", null],
    queryFn: async ({ signal }) => {
      const data = await fetchDetail(handle!);
      if (signal.aborted) {
        throw new DOMException("Aborted", "AbortError");
      }
      return data;
    },
    enabled: Boolean(handle),
  });

  useEffect(() => {
    if (query.isSuccess && handleRef.current === handle) {
      depsRef.current.markConnected();
    }
  }, [query.isSuccess, handle, query.dataUpdatedAt]);

  useEffect(() => {
    if (!handle) return;
    const error = query.isRefetchError || query.isError ? query.error : null;
    if (!error) return;
    if (error instanceof ApiError && error.status !== 404) {
      depsRef.current.applyConnectionError(error);
    }
  }, [query.isError, query.isRefetchError, query.error, handle]);

  const resumeOnOpen = useCallback(async (requestedHandle: string): Promise<boolean> => {
    try {
      const opResult = await executeOperation({
        task_handle: requestedHandle,
        action: "resume",
        request_id: requestId(),
      });
      if (handleRef.current !== requestedHandle) return false;
      if (opResult.ok && opResult.response.cockpit) {
        depsRef.current.applyCockpit(opResult.response.cockpit);
      }
      return opResult.ok;
    } catch {
      return false;
    }
  }, [executeOperation]);

  useEffect(() => {
    if (!handle) {
      resumedHandleRef.current = null;
      return;
    }
    if (resumedHandleRef.current === handle) return;
    resumedHandleRef.current = handle;
    void resumeOnOpen(handle).then((mutated) => {
      if (mutated && handleRef.current === handle) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.taskDetail(handle) });
      }
    });
  }, [handle, queryClient, resumeOnOpen]);

  const detail = useMemo((): RemoteResource<BrowserTaskDetail> => {
    if (!handle) {
      return { status: "loading", data: null, error: null };
    }
    const detailData = query.data;
    if (!detailData && (query.isPending || query.isFetching)) {
      return { status: "loading", data: null, error: null };
    }
    if (query.isRefetchError && detailData) {
      return {
        status: "stale",
        data: detailData,
        error: toDetailError(query.error),
      };
    }
    if (query.isError) {
      const detailError = toDetailError(query.error);
      if (detailData) {
        return { status: "stale", data: detailData, error: detailError };
      }
      return { status: "error", data: null, error: detailError };
    }
    if (detailData) {
      return { status: "ready", data: detailData, error: null };
    }
    return { status: "loading", data: null, error: null };
  }, [
    handle,
    query.isPending,
    query.isFetching,
    query.isError,
    query.isRefetchError,
    query.error,
    query.data,
  ]);

  const reload = useCallback(() => {
    const current = handleRef.current;
    if (!current) return;
    void queryClient.invalidateQueries({ queryKey: queryKeys.taskDetail(current) });
  }, [queryClient]);

  return { detail, reload };
}
