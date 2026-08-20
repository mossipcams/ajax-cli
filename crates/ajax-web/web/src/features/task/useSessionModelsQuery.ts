import { useQuery } from "@tanstack/react-query";
import {
  fetchSessionModels,
  normalizeSessionAgent,
  type SessionModelCatalog,
} from "./desiredModel";
import { queryKeys } from "@/shared/lib/queryClient";

export function useSessionModelsQuery(agent: string) {
  const harness = normalizeSessionAgent(agent);
  return useQuery({
    queryKey: queryKeys.sessionModels(harness),
    queryFn: () => fetchSessionModels(harness),
    staleTime: 0,
    retry: 1,
  });
}

export type { SessionModelCatalog };
