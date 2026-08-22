import { useQuery } from "@tanstack/react-query";
import { fetchDevDeploy } from "@/shared/lib/api";
import { queryKeys } from "@/shared/lib/queryClient";

const DEV_DEPLOY_POLL_MS = 1500;

export function useDevDeployQuery() {
  return useQuery({
    queryKey: queryKeys.devDeploy(),
    queryFn: ({ signal }) => fetchDevDeploy(signal),
    refetchInterval: (query) =>
      query.state.data?.deploy.active ? DEV_DEPLOY_POLL_MS : false,
  });
}
