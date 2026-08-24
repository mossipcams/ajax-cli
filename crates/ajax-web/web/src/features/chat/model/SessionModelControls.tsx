import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { Button } from "@/shared/ui/button";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import {
  fastApplyValue,
  modelConfigBooleanLiveOption,
  modelLiveOption,
  readLiveFastCurrent,
  readLiveSelectCurrent,
  thoughtLevelLiveOption,
} from "@/shared/lib/liveSessionConfig";
import { DEFAULT_SESSION_MODEL } from "@/features/task/public";

/** iOS shows :active on Effort / Fast then drops click when the model list's
 *  overflow layer covers those chips. Touch applies on pointerdown. */
function activateExtrasChip(
  event: { pointerType: string; preventDefault: () => void; stopPropagation: () => void },
  apply: () => void,
) {
  event.stopPropagation();
  if (event.pointerType === "mouse") return;
  event.preventDefault();
  apply();
}

interface PickerProps {
  agent?: string;
  confirmedModel?: string;
  options: LiveSessionConfigOption[];
  disabled?: boolean;
  onApply: (configId: string, value: string | boolean) => void;
}

/** Whether live model, effort, and Fast controls should be offered. */
export function hasSessionModelControls(
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

/** Dismissable refusal notice for config-option apply failures. */
export function SessionModelNotice({
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

export function SessionModelPickers({
  confirmedModel: _confirmedModel = "",
  options,
  disabled = false,
  onApply,
}: PickerProps) {
  const model = modelLiveOption(options);
  const thought = thoughtLevelLiveOption(options);
  const fast = modelConfigBooleanLiveOption(options);
  const thoughtChoices = thought?.choices ?? [];
  const showThought = thoughtChoices.length > 1;
  const showModel = Boolean(model?.choices.length);
  const showFast = Boolean(fast);

  if (!showModel && !showThought && !showFast) return null;

  const modelCurrent = model ? readLiveSelectCurrent(model) : undefined;
  const unlistedModel =
    modelCurrent && !model?.choices.some((choice) => choice.value === modelCurrent)
      ? modelCurrent
      : "";
  const fastOn = fast ? (readLiveFastCurrent(fast) ?? false) : false;

  return (
    <div className="session-config-pickers" data-testid="session-config-pickers">
      {showModel && model ? (
        <div className="session-model-catalog">
          <div
            className="model-picker"
            role="radiogroup"
            aria-label={model.name}
            data-testid="session-config-model"
          >
            {unlistedModel ? (
              // The host reports a model it does not advertise. Show it so the list
              // is never left with nothing selected; re-applying it is not offered
              // because the bridge only accepts advertised values.
              <button
                type="button"
                className="model-option is-selected"
                role="radio"
                aria-checked
                disabled
              >
                <span className="model-option-label">{unlistedModel}</span>
              </button>
            ) : null}
            {model.choices.map((choice) => {
              const selected = modelCurrent === choice.value;
              return (
                <button
                  key={choice.value}
                  type="button"
                  className={`model-option${selected ? " is-selected" : ""}`}
                  role="radio"
                  aria-checked={selected}
                  disabled={disabled}
                  onClick={() => onApply(model.id, choice.value)}
                >
                  <span className="model-option-label">{choice.name}</span>
                </button>
              );
            })}
          </div>
        </div>
      ) : null}

      {showThought && thought ? (
        <div className="session-config-group">
          <span className="field-label">{thought.name}</span>
          <div
            className="reasoning-picker"
            role="radiogroup"
            aria-label={thought.name}
            data-testid="session-config-thought"
          >
            {thoughtChoices.map((choice) => {
              const selected = readLiveSelectCurrent(thought) === choice.value;
              const apply = () => {
                if (!selected) onApply(thought.id, choice.value);
              };
              return (
                <button
                  key={choice.value}
                  type="button"
                  className={`reasoning-option${selected ? " is-selected" : ""}`}
                  role="radio"
                  aria-checked={selected}
                  disabled={disabled}
                  onPointerDown={(event) => activateExtrasChip(event, apply)}
                  onClick={apply}
                >
                  {choice.name}
                </button>
              );
            })}
          </div>
        </div>
      ) : null}

      {showFast && fast ? (
        <div className="session-config-group">
          <span className="field-label">{fast.name}</span>
          <div
            className="reasoning-picker"
            role="radiogroup"
            aria-label={fast.name}
            data-testid="session-config-fast"
          >
            {[
              { on: false, label: "Off" },
              { on: true, label: "On" },
            ].map((choice) => {
              const selected = choice.on === fastOn;
              const apply = () => {
                if (!selected) onApply(fast.id, fastApplyValue(fast, choice.on));
              };
              return (
              <button
                key={String(choice.on)}
                type="button"
                className={`reasoning-option${selected ? " is-selected" : ""}`}
                role="radio"
                aria-checked={selected}
                disabled={disabled}
                onPointerDown={(event) => activateExtrasChip(event, apply)}
                onClick={apply}
              >
                {choice.label}
              </button>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}

export interface SessionModelSheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  panelId: string;
  agent?: string;
  confirmedModel: string;
  options: LiveSessionConfigOption[];
  disabled?: boolean;
  onApply: (configId: string, value: string | boolean) => void;
}

/** Short label for the hotbar model control when a friendly name is unavailable. */
export function sessionModelControlLabel(
  confirmedModel: string,
  options?: LiveSessionConfigOption[],
): string {
  const trimmed = confirmedModel.trim();
  if (!trimmed || trimmed === DEFAULT_SESSION_MODEL) return "Auto";
  const model = modelLiveOption(options ?? []);
  const fromOption = model?.choices.find((choice) => choice.value === trimmed)?.name;
  if (fromOption) return fromOption;
  const tail = trimmed.replace(/^cursor-/, "").replace(/-/g, " ");
  return tail.length > 16 ? `${tail.slice(0, 14)}…` : tail;
}

export default function SessionModelControls({
  open,
  onOpenChange,
  panelId,
  agent,
  confirmedModel,
  options,
  disabled = false,
  onApply,
}: SessionModelSheetProps) {
  if (!open) return null;

  function close() {
    onOpenChange(false);
  }

  return (
    <FullscreenLayer zIndex={50}>
      <Sheet open onOpenChange={(next) => !next && close()}>
        <SheetContent
          asChild
          aria-describedby={undefined}
          onOpenAutoFocus={(event) => {
            event.preventDefault();
          }}
        >
          <div
            className="session-sheet-scrim"
            onPointerDown={(event) => {
              if (event.target === event.currentTarget) close();
            }}
          >
            <div
              className="session-model-switch-sheet"
              id={panelId}
              data-testid="model-switch-sheet"
              role="dialog"
              aria-modal="true"
              aria-label="Choose model"
            >
              <div className="session-sheet-header">
                <SheetTitle asChild>
                  <h2>Model</h2>
                </SheetTitle>
                <Button
                  type="button"
                  variant="secondary"
                  className="session-sheet-close"
                  data-testid="model-switch-close"
                  onClick={close}
                >
                  Close
                </Button>
              </div>

              <div className="session-model-switch-body" data-testid="model-switch-body">
                {/* The advertised list marks the running model itself; name it in
                    prose only when this harness advertises no model to select. */}
                {modelLiveOption(options)?.choices.length ? null : (
                  <p className="session-model-switch-current" data-testid="model-switch-current">
                    Current: {sessionModelControlLabel(confirmedModel, options)}
                  </p>
                )}
                <SessionModelPickers
                  agent={agent}
                  confirmedModel={confirmedModel}
                  options={options}
                  disabled={disabled}
                  onApply={onApply}
                />
              </div>
            </div>
          </div>
        </SheetContent>
      </Sheet>
    </FullscreenLayer>
  );
}

export interface SessionModelOpenButtonProps {
  panelId: string;
  label: string;
  disabled: boolean;
  expanded: boolean;
  onOpen: () => void;
}

export function SessionModelOpenButton({
  panelId,
  label,
  disabled,
  expanded,
  onOpen,
}: SessionModelOpenButtonProps) {
  return (
    <button
      type="button"
      className="session-composer-button session-composer-model"
      data-testid="session-model-open"
      aria-label="Choose model"
      aria-expanded={expanded}
      aria-controls={panelId}
      title={`Choose model — ${label}`}
      disabled={disabled}
      onMouseDown={(event) => event.preventDefault()}
      onClick={onOpen}
    >
      <svg
        className="session-composer-model-icon"
        viewBox="0 0 24 24"
        width="20"
        height="20"
        aria-hidden="true"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M12 3l1.5 4.5L18 9l-4.5 1.5L12 15l-1.5-4.5L6 9l4.5-1.5L12 3z" />
        <path d="M19 14l1 3 3 1-3 1-1 3-1-3-3-1 3-1 1-3z" />
      </svg>
    </button>
  );
}
