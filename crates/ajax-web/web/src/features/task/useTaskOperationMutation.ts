import { useMutation } from "@tanstack/react-query";
import { postOperation } from "@/shared/lib/api";

export type ExecuteTaskOperation = (
  params: Parameters<typeof postOperation>[0],
) => ReturnType<typeof postOperation>;

export function useTaskOperationMutation(): ExecuteTaskOperation {
  const mutation = useMutation({
    mutationFn: (params: Parameters<typeof postOperation>[0]) => postOperation(params),
    retry: false,
  });

  return (params) => mutation.mutateAsync(params);
}
