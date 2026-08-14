import { useEffect, useRef, useState } from "react";
import { fetchSessionModels, type SessionModelCatalog } from "./sessionModel";

interface Props {
  /** Harness whose own catalog to list. */
  agent: string;
  agentLabel: string;
  value: string;
  onChange: (model: string) => void;
  /** Called once with the harness default so callers can preselect it. */
  onCatalog?: (catalog: SessionModelCatalog) => void;
}

/**
 * The models one harness advertises. Cursor answers from its CLI; the bridge
 * harnesses answer from their own ACP handshake, so an empty list is a normal
 * outcome and means "let the harness pick".
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

  // Long catalogs scroll: bring the current choice into view rather than
  // opening on whatever happens to be first.
  useEffect(() => {
    if (!value) return;
    listRef.current
      ?.querySelector<HTMLElement>("[aria-checked='true']")
      // Optional call: jsdom has no scrollIntoView.
      ?.scrollIntoView?.({ block: "nearest" });
  }, [value, catalog]);

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

  return (
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
          className={`model-option${value === option.id ? " is-selected" : ""}`}
          role="radio"
          aria-checked={value === option.id}
          onClick={() => onChange(option.id)}
        >
          <span className="model-option-label">{option.label}</span>
          {option.id === catalog.default ? <span className="model-option-tag">Default</span> : null}
        </button>
      ))}
    </div>
  );
}
