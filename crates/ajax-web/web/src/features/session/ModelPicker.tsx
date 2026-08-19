import { useEffect, useRef, useState } from "react";
import { Button } from "@/shared/ui/button";
import {
  decodeModelSelection,
  encodeModelSelection,
  DEFAULT_SESSION_MODEL,
  type SessionModelCatalog,
} from "./sessionModel";
import { buildModelShortlist } from "./modelShortlist";
import { useSessionModelsQuery } from "./useSessionModelsQuery";

interface Props {
  /** Harness whose own catalog to list. */
  agent: string;
  agentLabel: string;
  /** Composite selection: `opus|effort=high`. */
  value: string;
  disabled?: boolean;
  onChange: (selection: string) => void;
  /** Called once with the harness default so callers can preselect it. */
  onCatalog?: (catalog: SessionModelCatalog) => void;
}

/**
 * The models one harness advertises, plus its reasoning level when that is a
 * separate choice. Cursor bakes the level into its model ids; the bridges
 * expose it as their own config option, so it needs its own list.
 */
export default function ModelPicker({
  agent,
  agentLabel,
  value,
  disabled = false,
  onChange,
  onCatalog,
}: Props) {
  const { data: catalog, isPending, isError, refetch } = useSessionModelsQuery(agent);
  const listRef = useRef<HTMLDivElement>(null);
  const catalogNotifiedRef = useRef<string | null>(null);
  const [showAll, setShowAll] = useState(false);

  useEffect(() => {
    if (!catalog || catalogNotifiedRef.current === agent) return;
    catalogNotifiedRef.current = agent;
    onCatalog?.(catalog);
  }, [agent, catalog, onCatalog]);

  const { model, options } = decodeModelSelection(value);

  useEffect(() => {
    setShowAll(false);
  }, [agent, catalog]);

  useEffect(() => {
    if (!model) return;
    listRef.current
      ?.querySelector<HTMLElement>("[aria-checked='true']")
      ?.scrollIntoView?.({ block: "nearest" });
  }, [model, catalog, showAll]);

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

  if (catalog.models.length === 0) {
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

  const reasoning = catalog.reasoning;
  const catalogIds = new Set(catalog.models.map((option) => option.id));
  const isKnownSelection =
    !model || model === DEFAULT_SESSION_MODEL || catalogIds.has(model);
  const unknownModel = model && !isKnownSelection;

  const { shortlist, hasMore } = buildModelShortlist(catalog.models, agent, {
    currentModelId: model || undefined,
    catalogDefault: catalog.default || undefined,
  });
  const shortlistIds = new Set(shortlist.map((option) => option.id));
  const remainingCatalog = catalog.models.filter((option) => !shortlistIds.has(option.id));
  const visibleModels = showAll ? [...shortlist, ...remainingCatalog] : shortlist;

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
        {visibleModels.map((option) => (
          <button
            key={option.id}
            type="button"
            className={`model-option${model === option.id ? " is-selected" : ""}`}
            role="radio"
            aria-checked={model === option.id}
            disabled={disabled}
            onClick={() => onChange(encodeModelSelection(option.id, options))}
          >
            <span className="model-option-label">{option.label}</span>
            {option.id === catalog.default ? (
              <span className="model-option-tag">Default</span>
            ) : null}
          </button>
        ))}
      </div>

      {hasMore ? (
        <button
          type="button"
          className="model-picker-toggle"
          data-testid="model-picker-toggle"
          onClick={() => setShowAll((open) => !open)}
        >
          {showAll ? "Show fewer" : "Show all"}
        </button>
      ) : null}

      {reasoning ? (
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
