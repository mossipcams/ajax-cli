import { useMutation } from "@tanstack/react-query";
import { swapTaskAgent } from "@/shared/lib/api";

export function useSwapTaskAgentMutation(handle: string, onSwapped?: () => void) {
  return useMutation({
    mutationFn: ({ agent, model }: { agent: string; model?: string }) =>
      swapTaskAgent(handle, agent, model),
    retry: false,
    onSuccess: (result) => {
      if (result.ok) onSwapped?.();
    },
  });
}
