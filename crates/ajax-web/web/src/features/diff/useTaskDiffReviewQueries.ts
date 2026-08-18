import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { ApiError, fetchTaskDiff, fetchTaskPullRequests } from "@/shared/lib/api";
import { queryKeys, type TaskDiffSource } from "@/shared/lib/queryClient";
import type { PullRequestView, TaskDiffView } from "@/shared/lib/types";

export type DiffReviewLoadState =
  | { status: "loading"; phase: "pull-requests" | "diff" }
  | { status: "error"; message: string }
  | {
      status: "ready";
      prs: PullRequestView[];
      diff: TaskDiffView;
      prListError?: string;
    };

function errorText(error: unknown, fallback: string): string {
  if (error instanceof ApiError) return error.message;
  if (error instanceof Error) return error.message;
  return fallback;
}

export function useTaskDiffReviewQueries(handle: string, selectedPr?: number) {
  const prQuery = useQuery({
    queryKey: queryKeys.taskPullRequests(handle),
    queryFn: () => fetchTaskPullRequests(handle),
  });

  const diffSource: TaskDiffSource | null = useMemo(() => {
    if (!prQuery.isFetched) return null;
    const prNumber = selectedPr ?? prQuery.data?.[0]?.number;
    if (prNumber !== undefined) return { pr: prNumber };
    return { local: true };
  }, [prQuery.isFetched, prQuery.data, selectedPr]);

  const diffQuery = useQuery({
    queryKey: diffSource
      ? queryKeys.taskDiff(handle, diffSource)
      : ["task-diff", handle, "pending"],
    queryFn: () => fetchTaskDiff(handle, diffSource!),
    enabled: diffSource !== null,
  });

  const state = useMemo((): DiffReviewLoadState => {
    if (!prQuery.isFetched) {
      return { status: "loading", phase: "pull-requests" };
    }
    if (diffQuery.isPending) {
      return { status: "loading", phase: "diff" };
    }
    if (diffQuery.isError) {
      return {
        status: "error",
        message: errorText(diffQuery.error, "Failed to load diff"),
      };
    }
    if (!diffQuery.data?.judgment) {
      return { status: "error", message: "Diff response missing judgment projection" };
    }
    const prListError = prQuery.isError
      ? errorText(prQuery.error, "Failed to load pull requests")
      : undefined;
    return {
      status: "ready",
      prs: prQuery.data ?? [],
      diff: diffQuery.data,
      prListError,
    };
  }, [
    prQuery.isFetched,
    prQuery.isError,
    prQuery.error,
    prQuery.data,
    diffQuery.isPending,
    diffQuery.isError,
    diffQuery.error,
    diffQuery.data,
  ]);

  return { state };
}
