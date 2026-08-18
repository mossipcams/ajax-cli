import { useCallback } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchVersion } from "@/shared/lib/api";
import { queryKeys } from "@/shared/lib/queryClient";

export function useVersionQuery(enabled = true) {
  return useQuery({
    queryKey: queryKeys.version(),
    queryFn: fetchVersion,
    enabled,
  });
}

export function useFetchVersion() {
  const queryClient = useQueryClient();
  return useCallback(
    () =>
      queryClient.fetchQuery({
        queryKey: queryKeys.version(),
        queryFn: fetchVersion,
      }),
    [queryClient],
  );
}
