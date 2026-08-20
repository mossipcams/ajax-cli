import { useEffect, useRef, useState } from "react";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import {
  fastApplyValue,
  modelConfigBooleanLiveOption,
  modelLiveOption,
  readLiveFastCurrent,
  readLiveSelectCurrent,
  thoughtLevelLiveOption,
} from "@/shared/lib/liveSessionConfig";

interface Props {
  /** Task harness; all harnesses use advertised configOptions when connected. */
  agent?: string;
  /** Confirmed host snapshot model (advertised id). */
  confirmedModel?: string;
  options: LiveSessionConfigOption[];
  disabled?: boolean;
  onApply: (configId: string, value: string | boolean) => void;
}

/** Whether live model/effort controls should be offered in the model switch sheet. */
export function hasConfigPickerControls(
  _agent: string | undefined,
  options: LiveSessionConfigOption[],
): boolean {
  const model = modelLiveOption(options);
  const thought = thoughtLevelLiveOption(options);
  const fast = modelConfigBooleanLiveOption(options);
  const thoughtChoices = thought?.choices ?? [];
  const showThought = thoughtChoices.length > 1;
  const showModel = Boolean(model?.choices.length);
  const showFast = Boolean(fast);
  return showModel || showThought || showFast;
}

/**
 * Live model/effort controls for the model switch sheet.
 * All harnesses bind to advertised configOptions; picks send configId+value.
 * Pessimistic: controls show confirmed snapshot values until the host updates.
 */
export default function ConfigPickers({
  confirmedModel: _confirmedModel = "",
  options,
  disabled = false,
  onApply,
}: Props) {
  const model = modelLiveOption(options);
  const thought = thoughtLevelLiveOption(options);
  const fast = modelConfigBooleanLiveOption(options);
  const thoughtChoices = thought?.choices ?? [];
  const showThought = thoughtChoices.length > 1;
  const showModel = Boolean(model?.choices.length);
  const showFast = Boolean(fast);

  if (!showModel && !showThought && !showFast) return null;

  return (
    <div className="session-config-pickers" data-testid="session-config-pickers">
      {showModel && model ? (
        <label className="session-config-chip">
          <span className="session-config-chip-label">{model.name}</span>
          <select
            data-testid="session-config-model"
            aria-label={model.name}
            disabled={disabled}
            value={readLiveSelectCurrent(model) ?? ""}
            onChange={(event) => onApply(model.id, event.target.value)}
          >
            {model.choices.map((choice) => (
              <option key={choice.value} value={choice.value}>
                {choice.name}
              </option>
            ))}
          </select>
        </label>
      ) : null}

      {showThought && thought ? (
        <div
          className="session-config-segment"
          role="radiogroup"
          aria-label={thought.name}
          data-testid="session-config-thought"
        >
          {thoughtChoices.map((choice) => {
            const current = readLiveSelectCurrent(thought);
            const selected = current === choice.value;
            return (
              <button
                key={choice.value}
                type="button"
                className={`session-config-segment-option${selected ? " is-selected" : ""}`}
                role="radio"
                aria-checked={selected}
                disabled={disabled}
                onClick={() => onApply(thought.id, choice.value)}
              >
                {choice.name}
              </button>
            );
          })}
        </div>
      ) : null}

      {showFast && fast ? (
        <label className="session-config-chip">
          <span className="session-config-chip-label">{fast.name}</span>
          <input
            type="checkbox"
            data-testid="session-config-fast"
            aria-label={fast.name}
            disabled={disabled}
            checked={readLiveFastCurrent(fast) ?? false}
            onChange={(event) => onApply(fast.id, fastApplyValue(fast, event.target.checked))}
          />
        </label>
      ) : null}
    </div>
  );
}

/** Dismissable refusal notice for config-option apply failures. */
export function ConfigPickerNotice({
  message,
  onDismiss,
}: {
  message: string;
  onDismiss: () => void;
}) {
  return (
    <div className="session-config-notice" data-testid="session-config-notice" role="alert">
      <p>{message}</p>
      <button type="button" onClick={onDismiss}>
        Dismiss
      </button>
    </div>
  );
}

export function useConfigPickerNotice() {
  const [notice, setNotice] = useState<string | null>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  function showNotice(message: string) {
    setNotice(message);
  }

  function dismissNotice() {
    setNotice(null);
  }

  return { notice, showNotice, dismissNotice };
}
