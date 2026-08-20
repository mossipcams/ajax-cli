import { QueryClient } from "@tanstack/react-query";

export type TaskDiffSource = { pr: number } | { local: true };

export const queryKeys = {
  taskDetail: (handle: string) => ["task-detail", handle] as const,
  taskPullRequests: (handle: string) => ["task-pull-requests", handle] as const,
  taskDiff: (handle: string, source: TaskDiffSource) =>
    ["task-diff", handle, source] as const,
  sessionModels: (agent: string) => ["session-models", agent] as const,
  sessionOptionCatalog: (agent: string) => ["session-option-catalog", agent] as const,
  version: () => ["version"] as const,
  devDeploy: () => ["dev-deploy"] as const,
};

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
        staleTime: 0,
      },
      mutations: {
        retry: false,
      },
    },
  });
}

/** Singleton for the live shell; tests use `createQueryClient()` per render. */
export const queryClient = createQueryClient();
