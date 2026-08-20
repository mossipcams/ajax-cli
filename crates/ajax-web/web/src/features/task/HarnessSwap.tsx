import { useState } from "react";
import { Button } from "@/shared/ui/button";
import ModelPicker from "./ModelPicker";
import { DEFAULT_SESSION_MODEL } from "./desiredModel";
import { useSwapTaskAgentMutation } from "./useSwapTaskAgentMutation";
import { AGENTS, agentLabel } from "./agents";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";

interface Props {
  handle: string;
  /** Harness the task runs on now, as reported by the task detail. */
  currentAgent: string;
  /** Live session model from the host snapshot (catalog id or `auto`). */
  currentModel?: string;
  /** Advertised ACP config options for the connected session. */
  liveConfigOptions?: LiveSessionConfigOption[];
  disabled?: boolean;
  onSwapped?: () => void;
}

/**
 * Cross-harness Switch only when a session is connected (AoE contract).
 * Same-harness model changes use the composer-footer picker, not Switch.
 */
export default function HarnessSwap({
  handle,
  currentAgent,
  currentModel = DEFAULT_SESSION_MODEL,
  liveConfigOptions,
  disabled = false,
  onSwapped,
}: Props) {
  const [open, setOpen] = useState(false);
  const [agent, setAgent] = useState(currentAgent);
  const [model, setModel] = useState(currentModel);
  const [error, setError] = useState<string | null>(null);

  const connected = (liveConfigOptions?.length ?? 0) > 0;
  const showModelPicker = !connected || agent !== currentAgent;

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
    if (connected && agent === currentAgent) {
      setError("Same-harness model changes use in-session config chips, not Switch");
      return;
    }
    try {
      const result = await swapMutation.mutateAsync({
        agent,
        model: showModelPicker ? persistedModel(model) : undefined,
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
            onClick={() => setAgent(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>

      {showModelPicker ? (
        <>
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
        </>
      ) : (
        <p className="sheet-note" data-testid="harness-swap-harness-only">
          Model changes for this harness use the composer config chips.
        </p>
      )}

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
