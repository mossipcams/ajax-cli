import { useEffect, useRef, useState } from "react";
import {
  decodeModelSelection,
  encodeModelSelection,
  fetchSessionModels,
  type SessionModelCatalog,
} from "./sessionModel";

interface Props {
  /** Harness whose own catalog to list. */
  agent: string;
  agentLabel: string;
  /** Composite selection: `opus|effort=high`. */
  value: string;
  onChange: (selection: string) => void;
  /** Called once with the harness default so callers can preselect it. */
  onCatalog?: (catalog: SessionModelCatalog) => void;
}

/**
 * The models one harness advertises, plus its reasoning level when that is a
 * separate choice. Cursor bakes the level into its model ids; the bridges
 * expose it as their own config option, so it needs its own list.
 */
export default function ModelPicker({ agent, agentLabel, value, onChange, onCatalog }: Props) {
  const [catalog, setCatalog] = useState<SessionModelCatalog | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    setCatalog(null);
    void fetchSessionModels(agent).then((next) => {
      if (cancelled) return;
      setCatalog(next);
      onCatalog?.(next);
    });
    return () => {
      cancelled = true;
    };
    // onCatalog is a notification, not an input: re-fetching on identity change
    // would restart the handshake on every parent render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agent]);

  const { model, options } = decodeModelSelection(value);

  // Long catalogs scroll: bring the current choice into view rather than
  // opening on whatever happens to be first.
  useEffect(() => {
    if (!model) return;
    listRef.current
      ?.querySelector<HTMLElement>("[aria-checked='true']")
      // Optional call: jsdom has no scrollIntoView.
      ?.scrollIntoView?.({ block: "nearest" });
  }, [model, catalog]);

  if (catalog === null) {
    return <p className="sheet-note">Reading models from {agentLabel}…</p>;
  }

  if (catalog.models.length === 0) {
    return (
      <p className="sheet-note">
        {agentLabel} lists no models here; it will start on its own default.
      </p>
    );
  }

  const reasoning = catalog.reasoning;

  return (
    <>
      <div
        className="model-picker"
        role="radiogroup"
        aria-label={`${agentLabel} models`}
        ref={listRef}
      >
        {catalog.models.map((option) => (
          <button
            key={option.id}
            type="button"
            className={`model-option${model === option.id ? " is-selected" : ""}`}
            role="radio"
            aria-checked={model === option.id}
            onClick={() => onChange(encodeModelSelection(option.id, options))}
          >
            <span className="model-option-label">{option.label}</span>
            {option.id === catalog.default ? (
              <span className="model-option-tag">Default</span>
            ) : null}
          </button>
        ))}
      </div>

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
