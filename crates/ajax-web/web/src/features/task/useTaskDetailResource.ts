import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, fetchDetail, postOperation, requestId } from "@/shared/lib/api";
import type { BrowserCockpitView, BrowserTaskDetail, RemoteResource } from "@/shared/lib/types";

export type TaskDetailResourceDeps = {
  applyCockpit: (next: BrowserCockpitView) => void;
  applyConnectionError: (error: unknown) => void;
  markConnected: () => void;
};

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

  const loadGenRef = useRef(0);
  const resumedHandleRef = useRef<string | null>(null);

  const [detail, setDetail] = useState<RemoteResource<BrowserTaskDetail>>({
    status: "loading",
    data: null,
    error: null,
  });

  const loadDetail = useCallback(async (requestedHandle: string) => {
    const gen = ++loadGenRef.current;
    try {
      const next = await fetchDetail(requestedHandle);
      if (handleRef.current !== requestedHandle || gen !== loadGenRef.current) return;
      setDetail({ status: "ready", data: next, error: null });
      depsRef.current.markConnected();
    } catch (error) {
      if (handleRef.current !== requestedHandle || gen !== loadGenRef.current) return;
      const detailError =
        error instanceof ApiError
          ? error
          : new ApiError("incompatible", error instanceof Error ? error.message : String(error));
      // #861: missing task is a detail error, not a cockpit disconnect
      if (error instanceof ApiError && error.status !== 404) {
        depsRef.current.applyConnectionError(error);
      }
      setDetail((prev) => {
        if (prev.status === "ready" || prev.status === "stale") {
          return { status: "stale", data: prev.data, error: detailError };
        }
        return { status: "error", data: null, error: detailError };
      });
    }
  }, []);

  const resumeOnOpen = useCallback(async (requestedHandle: string): Promise<boolean> => {
    try {
      const opResult = await postOperation({
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
  }, []);

  const reload = useCallback(() => {
    const current = handleRef.current;
    if (!current) return;
    void loadDetail(current);
  }, [loadDetail]);

  useEffect(() => {
    if (!handle) {
      resumedHandleRef.current = null;
      setDetail({ status: "loading", data: null, error: null });
      return;
    }
    setDetail({ status: "loading", data: null, error: null });
    void loadDetail(handle);
    if (resumedHandleRef.current === handle) return;
    resumedHandleRef.current = handle;
    void resumeOnOpen(handle).then((mutated) => {
      if (mutated && handleRef.current === handle) {
        void loadDetail(handle);
      }
    });
  }, [handle, loadDetail, resumeOnOpen]);

  return { detail, reload };
}
