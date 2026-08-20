import { useEffect, useRef } from "react";
import { Button } from "@/shared/ui/button";
import {
  buildCursorDisplayModels,
  decodeCursorPipeOrCatalogId,
  decodeCursorSelection,
  decodeModelSelection,
  defaultCursorEffort,
  effortOptionLabel,
  encodeCursorSelection,
  encodeModelSelection,
  DEFAULT_SESSION_MODEL,
  normalizeSessionAgent,
  type SessionModelCatalog,
} from "./desiredModel";
import { useSessionModelsQuery } from "./useSessionModelsQuery";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import {
  encodeDesiredPinWithLiveSelection,
  modelConfigBooleanLiveOption,
  readLiveBooleanCurrent,
  readLiveSelectCurrent,
  thoughtLevelLiveOption,
} from "@/shared/lib/liveSessionConfig";

interface Props {
  /** Harness whose own catalog to list. */
  agent: string;
  agentLabel: string;
  /** Composite selection: catalog id, or `opus|effort=high` for bridges. */
  value: string;
  /** Live advertised ACP config options (connected session). */
  liveConfigOptions?: LiveSessionConfigOption[];
  disabled?: boolean;
  onChange: (selection: string) => void;
  /** Called once with the harness default so callers can preselect it. */
  onCatalog?: (catalog: SessionModelCatalog) => void;
}

/**
 * Full harness catalog for model choice, plus effort / Fast when multiple levels
 * exist. Live sessionConfigOptions seed the current pin and supply in-band apply
 * ids; they do not replace the catalog model list.
 */
export default function ModelPicker({
  agent,
  agentLabel,
  value,
  liveConfigOptions,
  disabled = false,
  onChange,
  onCatalog,
}: Props) {
  const { data: catalog, isPending, isError, refetch } = useSessionModelsQuery(agent);
  const listRef = useRef<HTMLDivElement>(null);
  const catalogNotifiedRef = useRef<string | null>(null);
  const isCursor = normalizeSessionAgent(agent) === "cursor";

  useEffect(() => {
    if (!catalog || catalogNotifiedRef.current === agent) return;
    catalogNotifiedRef.current = agent;
    onCatalog?.(catalog);
  }, [agent, catalog, onCatalog]);

  const { model, options } = decodeModelSelection(value);

  useEffect(() => {
    if (!model) return;
    listRef.current
      ?.querySelector<HTMLElement>("[aria-checked='true']")
      ?.scrollIntoView?.({ block: "nearest" });
  }, [model, catalog]);

  if (isError) {
    return (
      <p className="sheet-error" data-testid="model-catalog-error">
        Could not read models from {agentLabel}.{" "}
        <Button type="button" variant="secondary" onClick={() => void refetch()}>
          Retry
        </Button>
      </p>
    );
  }

  if (isPending || catalog === undefined) {
    return <p className="sheet-note">Reading models from {agentLabel}…</p>;
  }

  const activeCatalog = catalog;

  if (activeCatalog.models.length === 0) {
    return catalog.error ? (
      <p className="sheet-error" data-testid="model-catalog-error">
        {catalog.error}
      </p>
    ) : (
      <p className="sheet-note">
        {agentLabel} lists no models here; it will start on its own default.
      </p>
    );
  }

  const liveOptions = liveConfigOptions ?? [];
  const thoughtOption = liveOptions.length ? thoughtLevelLiveOption(liveOptions) : undefined;
  const fastOption = liveOptions.length ? modelConfigBooleanLiveOption(liveOptions) : undefined;
  const showLiveEffort = !!thoughtOption && thoughtOption.choices.length > 1;
  const showLiveFast = !!fastOption;
  const selectedThought = thoughtOption
    ? options[thoughtOption.id] ?? readLiveSelectCurrent(thoughtOption)
    : undefined;
  const selectedFast = fastOption
    ? options[fastOption.id] === "true" ||
      (options[fastOption.id] === undefined ? readLiveBooleanCurrent(fastOption) : false)
    : undefined;

  const reasoning = catalog.reasoning;
  const catalogIds = new Set(catalog.models.map((option) => option.id));
  const cursorIntent = isCursor && model ? decodeCursorPipeOrCatalogId(model) : null;
  const isKnownSelection =
    !model ||
    model === DEFAULT_SESSION_MODEL ||
    catalogIds.has(model) ||
    (isCursor && cursorIntent !== null && catalogIds.has(cursorIntent.base));
  const unknownModel = model && !isKnownSelection;

  const displayModels = isCursor ? buildCursorDisplayModels(catalog.models) : [];
  const cursorSelection =
    isCursor && model && model !== DEFAULT_SESSION_MODEL
      ? decodeCursorSelection(model, displayModels)
      : null;
  const selectedCursorRow =
    cursorSelection && displayModels.find((row) => row.base === cursorSelection.base);

  const showCatalogEffort =
    !showLiveEffort && isCursor && selectedCursorRow && selectedCursorRow.efforts.length > 1;
  const showCatalogFast = !showLiveFast && isCursor && selectedCursorRow?.hasFast;

  const autoOption = catalog.models.find(
    (option) => option.id === DEFAULT_SESSION_MODEL || option.id === "auto",
  );

  function emitLive(selection: {
    model?: string;
    thoughtLevel?: string;
    fast?: boolean;
  }) {
    onChange(encodeDesiredPinWithLiveSelection(liveOptions, selection));
  }

  function emitCursorSelection(base: string, effort: string | undefined, fast: boolean) {
    const row = displayModels.find((entry) => entry.base === base);
    if (!row) return;
    if (liveOptions.length) {
      emitLive({
        model: base,
        thoughtLevel: effort ?? selectedThought,
        fast: fastOption ? fast : undefined,
      });
      return;
    }
    onChange(encodeCursorSelection(base, effort, fast, row));
  }

  function selectCursorBase(base: string) {
    const row = displayModels.find((entry) => entry.base === base);
    if (!row) return;
    emitCursorSelection(base, defaultCursorEffort(row, activeCatalog.default), false);
  }

  function selectCatalogModel(optionId: string) {
    if (liveOptions.length) {
      emitLive({ model: optionId, thoughtLevel: selectedThought, fast: selectedFast });
      return;
    }
    onChange(encodeModelSelection(optionId, options));
  }

  return (
    <>
      <div
        className="model-picker"
        role="radiogroup"
        aria-label={`${agentLabel} models`}
        ref={listRef}
      >
        {unknownModel ? (
          <button
            key={`unknown-${model}`}
            type="button"
            className="model-option is-selected"
            role="radio"
            aria-checked
            disabled={disabled}
            onClick={() => onChange(value)}
          >
            <span className="model-option-label">{model}</span>
          </button>
        ) : null}

        {isCursor ? (
          <>
            {autoOption ? (
              <button
                key={autoOption.id}
                type="button"
                className={`model-option${model === autoOption.id || model === DEFAULT_SESSION_MODEL ? " is-selected" : ""}`}
                role="radio"
                aria-checked={model === autoOption.id || model === DEFAULT_SESSION_MODEL}
                disabled={disabled}
                onClick={() => selectCatalogModel(autoOption.id)}
              >
                <span className="model-option-label">{autoOption.label}</span>
                {autoOption.id === catalog.default ? (
                  <span className="model-option-tag">Default</span>
                ) : null}
              </button>
            ) : null}
            {displayModels.map((row) => (
              <button
                key={row.base}
                type="button"
                className={`model-option${cursorSelection?.base === row.base ? " is-selected" : ""}`}
                role="radio"
                aria-checked={cursorSelection?.base === row.base}
                disabled={disabled}
                onClick={() => selectCursorBase(row.base)}
              >
                <span className="model-option-label">{row.label}</span>
              </button>
            ))}
          </>
        ) : (
          catalog.models.map((option) => (
            <button
              key={option.id}
              type="button"
              className={`model-option${model === option.id ? " is-selected" : ""}`}
              role="radio"
              aria-checked={model === option.id}
              disabled={disabled}
              onClick={() => selectCatalogModel(option.id)}
            >
              <span className="model-option-label">{option.label}</span>
              {option.id === catalog.default ? (
                <span className="model-option-tag">Default</span>
              ) : null}
            </button>
          ))
        )}
      </div>

      {showLiveEffort && thoughtOption ? (
        <>
          <span className="field-label" id="live-thought-label">
            {thoughtOption.name}
          </span>
          <div
            className="reasoning-picker"
            role="radiogroup"
            aria-labelledby="live-thought-label"
            data-testid="live-thought-level"
          >
            {thoughtOption.choices.map((choice) => (
              <button
                key={choice.value}
                type="button"
                className={`reasoning-option${selectedThought === choice.value ? " is-selected" : ""}`}
                role="radio"
                aria-checked={selectedThought === choice.value}
                disabled={disabled}
                onClick={() =>
                  emitLive({
                    model: cursorSelection?.base ?? model ?? undefined,
                    thoughtLevel: choice.value,
                    fast: selectedFast,
                  })
                }
              >
                {choice.name}
              </button>
            ))}
          </div>
        </>
      ) : null}

      {showCatalogEffort && selectedCursorRow ? (
        <>
          <span className="field-label" id="model-effort-label">
            Effort
          </span>
          <div
            className="reasoning-picker"
            role="radiogroup"
            aria-labelledby="model-effort-label"
            data-testid="model-effort"
          >
            {selectedCursorRow.efforts.map((effort) => (
              <button
                key={effort}
                type="button"
                className={`reasoning-option${cursorSelection?.effort === effort ? " is-selected" : ""}`}
                role="radio"
                aria-checked={cursorSelection?.effort === effort}
                disabled={disabled}
                onClick={() =>
                  emitCursorSelection(
                    selectedCursorRow.base,
                    effort,
                    cursorSelection?.fast ?? false,
                  )
                }
              >
                {effortOptionLabel(effort)}
              </button>
            ))}
          </div>
        </>
      ) : null}

      {showLiveFast && fastOption ? (
        <>
          <span className="field-label" id="live-fast-label">
            {fastOption.name}
          </span>
          <div
            className="reasoning-picker"
            role="radiogroup"
            aria-labelledby="live-fast-label"
            data-testid="live-model-fast"
          >
            {[
              { id: false, label: "Off" },
              { id: true, label: "On" },
            ].map((option) => {
              const fastOn = selectedFast ?? false;
              const selected = option.id === fastOn;
              return (
                <button
                  key={String(option.id)}
                  type="button"
                  className={`reasoning-option${selected ? " is-selected" : ""}`}
                  role="radio"
                  aria-checked={selected}
                  disabled={disabled}
                  onClick={() =>
                    emitLive({
                      model: cursorSelection?.base ?? model ?? undefined,
                      thoughtLevel: selectedThought,
                      fast: option.id,
                    })
                  }
                >
                  {option.label}
                </button>
              );
            })}
          </div>
        </>
      ) : null}

      {showCatalogFast && selectedCursorRow ? (
        <>
          <span className="field-label" id="model-fast-label">
            Fast
          </span>
          <div
            className="reasoning-picker"
            role="radiogroup"
            aria-labelledby="model-fast-label"
            data-testid="model-fast"
          >
            {[
              { id: "false", label: "Off" },
              { id: "true", label: "On" },
            ].map((option) => {
              const fastOn = cursorSelection?.fast ?? false;
              const selected = option.id === "true" ? fastOn : !fastOn;
              return (
                <button
                  key={option.id}
                  type="button"
                  className={`reasoning-option${selected ? " is-selected" : ""}`}
                  role="radio"
                  aria-checked={selected}
                  disabled={disabled}
                  onClick={() =>
                    emitCursorSelection(
                      selectedCursorRow.base,
                      cursorSelection?.effort ??
                        defaultCursorEffort(selectedCursorRow, catalog.default),
                      option.id === "true",
                    )
                  }
                >
                  {option.label}
                </button>
              );
            })}
          </div>
        </>
      ) : null}

      {reasoning && !showLiveEffort ? (
        <>
          <span className="field-label" id="model-reasoning-label">
            {reasoning.label}
          </span>
          <div
            className="reasoning-picker"
            role="radiogroup"
            aria-labelledby="model-reasoning-label"
            data-testid="model-reasoning"
          >
            {reasoning.options.map((option) => {
              const current = options[reasoning.id] ?? reasoning.default;
              return (
                <button
                  key={option.id}
                  type="button"
                  className={`reasoning-option${current === option.id ? " is-selected" : ""}`}
                  role="radio"
                  aria-checked={current === option.id}
                  disabled={disabled}
                  onClick={() =>
                    onChange(
                      encodeModelSelection(model || catalog.default, {
                        ...options,
                        [reasoning.id]: option.id,
                      }),
                    )
                  }
                >
                  {option.label}
                </button>
              );
            })}
          </div>
        </>
      ) : null}
    </>
  );
}
