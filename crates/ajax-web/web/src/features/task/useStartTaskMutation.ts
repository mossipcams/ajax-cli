import { useMutation } from "@tanstack/react-query";
import { requestId, startTask } from "@/shared/lib/api";

type StartTaskInput = Omit<Parameters<typeof startTask>[0], "request_id">;

export function useStartTaskMutation() {
  return useMutation({
    mutationFn: (params: StartTaskInput) =>
      startTask({ ...params, request_id: requestId() }),
    retry: false,
  });
}
