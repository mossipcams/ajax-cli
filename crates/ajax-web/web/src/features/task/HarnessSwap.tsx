import { useState } from "react";
import { Button } from "@/shared/ui/button";
import ModelPicker from "@/features/session/ModelPicker";
import { DEFAULT_SESSION_MODEL } from "@/features/session/sessionModel";
import { useSwapTaskAgentMutation } from "./useSwapTaskAgentMutation";
import { AGENTS, agentLabel } from "./agents";

interface Props {
  handle: string;
  /** Harness the task runs on now, as reported by the task detail. */
  currentAgent: string;
  /** Live session model from the host snapshot (catalog id or `auto`). */
  currentModel?: string;
  disabled?: boolean;
  onSwapped?: () => void;
}

/**
 * Move a running task to another harness. Only ACP-started tasks can move —
 * the backend refuses a task whose agent is live in its tmux pane, because
 * rewriting the registry under it would not stop that process.
 */
export default function HarnessSwap({
  handle,
  currentAgent,
  currentModel = DEFAULT_SESSION_MODEL,
  disabled = false,
  onSwapped,
}: Props) {
  const [open, setOpen] = useState(false);
  const [agent, setAgent] = useState(currentAgent);
  const [model, setModel] = useState(currentModel);
  const [error, setError] = useState<string | null>(null);

  const swapMutation = useSwapTaskAgentMutation(handle, () => {
    setOpen(false);
    onSwapped?.();
  });

  function persistedModel(selection: string): string | undefined {
    const trimmed = selection.trim();
    if (!trimmed || trimmed === DEFAULT_SESSION_MODEL) return undefined;
    return trimmed;
  }

  async function apply() {
    setError(null);
    try {
      const result = await swapMutation.mutateAsync({
        agent,
        model: persistedModel(model),
      });
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
            setModel(currentModel);
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
            onClick={() => {
              setAgent(option.value);
              setModel("");
            }}
          >
            {option.label}
          </button>
        ))}
      </div>

      <span className="field-label" id="harness-swap-model">
        Model
      </span>
      <ModelPicker
        agent={agent}
        agentLabel={agentLabel(agent)}
        value={model}
        disabled={disabled || swapMutation.isPending}
        onChange={setModel}
        onCatalog={(catalog) => setModel((current) => current || catalog.default)}
      />

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
