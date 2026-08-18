import { useQuery } from "@tanstack/react-query";
import { fetchSessionModels, type SessionModelCatalog } from "./sessionModel";
import { queryKeys } from "@/shared/lib/queryClient";

export function useSessionModelsQuery(agent: string) {
  return useQuery({
    queryKey: queryKeys.sessionModels(agent),
    queryFn: () => fetchSessionModels(agent),
  });
}

export type { SessionModelCatalog };
