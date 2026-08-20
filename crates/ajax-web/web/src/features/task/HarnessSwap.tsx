import { useState } from "react";
import { Button } from "@/shared/ui/button";
import { useSwapTaskAgentMutation } from "./useSwapTaskAgentMutation";
import { AGENTS, agentLabel } from "./agents";

interface Props {
  handle: string;
  /** Harness the task runs on now, as reported by the task detail. */
  currentAgent: string;
  disabled?: boolean;
  onSwapped?: () => void;
}

/**
 * Cross-harness Switch changes only the harness (AoE contract).
 * Same-harness model changes use the composer-footer picker, not Switch.
 */
export default function HarnessSwap({
  handle,
  currentAgent,
  disabled = false,
  onSwapped,
}: Props) {
  const [open, setOpen] = useState(false);
  const [agent, setAgent] = useState(currentAgent);
  const [error, setError] = useState<string | null>(null);

  const swapMutation = useSwapTaskAgentMutation(handle, () => {
    setOpen(false);
    onSwapped?.();
  });

  async function apply() {
    setError(null);
    if (agent === currentAgent) {
      setError("Same-harness model changes use in-session config chips, not Switch");
      return;
    }
    try {
      const result = await swapMutation.mutateAsync({ agent });
      if (!result.ok) {
        setError(result.error?.message ?? "Could not switch harness");
      }
    } catch {
      setError("Could not switch harness — network error");
    }
  }

  if (!open) {
    return (
      <div className="harness-swap" data-testid="harness-swap">
        <span className="harness-swap-current">Harness — {agentLabel(currentAgent)}</span>
        <button
          type="button"
          className="harness-swap-open"
          data-testid="harness-swap-open"
          onClick={() => {
            setAgent(currentAgent);
            setOpen(true);
          }}
        >
          Switch
        </button>
      </div>
    );
  }

  return (
    <div className="harness-swap is-open" data-testid="harness-swap">
      <span className="field-label" id="harness-swap-agent">
        Harness
      </span>
      <div className="agent-picker" role="radiogroup" aria-labelledby="harness-swap-agent">
        {AGENTS.map((option) => (
          <button
            key={option.value}
            type="button"
            className={`agent-option${agent === option.value ? " is-selected" : ""}`}
            role="radio"
            aria-checked={agent === option.value}
            disabled={option.value === currentAgent}
            onClick={() => setAgent(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>

      <p className="sheet-note" data-testid="harness-swap-harness-only">
        The new harness starts with Auto. Change its model after connecting.
      </p>

      {error ? (
        <p className="diff-status diff-error" data-testid="harness-swap-error">
          {error}
        </p>
      ) : null}

      <div className="sheet-actions">
        <Button type="button" variant="secondary" onClick={() => setOpen(false)}>
          Cancel
        </Button>
        <Button
          type="button"
          variant="default"
          disabled={disabled || swapMutation.isPending}
          onClick={() => void apply()}
          data-testid="harness-swap-apply"
        >
          {swapMutation.isPending ? "Switching…" : "Switch"}
        </Button>
      </div>
    </div>
  );
}
