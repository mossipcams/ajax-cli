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

  const [detail, setDetail] = useState<RemoteResource<BrowserTaskDetail>>({
    status: "loading",
    data: null,
    error: null,
  });

  const loadDetail = useCallback(async (requestedHandle: string) => {
    try {
      const next = await fetchDetail(requestedHandle);
      if (handleRef.current !== requestedHandle) return;
      setDetail({ status: "ready", data: next, error: null });
      depsRef.current.markConnected();
    } catch (error) {
      if (handleRef.current !== requestedHandle) return;
      if (!(error instanceof ApiError)) return;
      depsRef.current.applyConnectionError(error);
      setDetail((prev) => {
        if (prev.status === "ready" || prev.status === "stale") {
          return { status: "stale", data: prev.data, error };
        }
        // ponytail: network failures leave the outlet on the skeleton, matching
        // the pre-hook null-detail behavior the App shell tests still assert.
        if (error.kind === "network" && prev.status === "loading") {
          return prev;
        }
        return { status: "error", data: null, error };
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
      setDetail({ status: "loading", data: null, error: null });
      return;
    }
    setDetail({ status: "loading", data: null, error: null });
    void loadDetail(handle);
    void resumeOnOpen(handle).then((mutated) => {
      if (mutated && handleRef.current === handle) {
        void loadDetail(handle);
      }
    });
  }, [handle, loadDetail, resumeOnOpen]);

  return { detail, reload };
}
