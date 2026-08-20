import { useQuery } from "@tanstack/react-query";
import {
  fetchSessionOptionCatalog,
  normalizeSessionAgent,
  type SessionOptionCatalog,
} from "./desiredModel";
import { queryKeys } from "@/shared/lib/queryClient";

export function useOptionCatalogQuery(agent: string, options?: { enabled?: boolean }) {
  const harness = normalizeSessionAgent(agent);
  return useQuery({
    queryKey: queryKeys.sessionOptionCatalog(harness),
    queryFn: () => fetchSessionOptionCatalog(harness),
    staleTime: 0,
    retry: 1,
    enabled: options?.enabled ?? true,
  });
}

export type { SessionOptionCatalog };
