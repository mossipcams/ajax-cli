import { useMemo, useState } from "react";
import { agentLabel } from "@/features/task/agents";
import ModelPicker from "./ModelPicker";
import {
  decodeModelSelection,
  DEFAULT_SESSION_MODEL,
  normalizeSessionAgent,
} from "./sessionModel";
import { useSessionModelsQuery } from "./useSessionModelsQuery";

interface Props {
  id: string;
  /** Composite selection from the host, e.g. `opus|effort=high`. */
  value: string;
  disabled?: boolean;
  /** Harness whose catalog to list. */
  agent?: string;
  onChange: (model: string) => void;
}

function currentModelLabel(
  value: string,
  catalog: ReturnType<typeof useSessionModelsQuery>["data"],
): string {
  const { model, options } = decodeModelSelection(value);
  if (!catalog) return model || "…";

  const catalogIds = new Set(catalog.models.map((option) => option.id));
  const known =
    !model || model === DEFAULT_SESSION_MODEL || catalogIds.has(model);
  const modelLabel = known
    ? (catalog.models.find((option) => option.id === model)?.label ??
      (model === DEFAULT_SESSION_MODEL ? "Auto" : model || catalog.default || "Default"))
    : model;

  const reasoning = catalog.reasoning;
  if (!reasoning) return modelLabel;

  const level = options[reasoning.id] ?? reasoning.default;
  const levelLabel =
    reasoning.options.find((option) => option.id === level)?.label ?? level;
  return `${modelLabel} · ${levelLabel}`;
}

/** In-session model control for task details — summary row with catalog on demand. */
export default function SessionModelSelect({
  id,
  value,
  disabled,
  agent = "cursor",
  onChange,
}: Props) {
  const [catalogOpen, setCatalogOpen] = useState(false);
  const harness = normalizeSessionAgent(agent);
  const { data: catalog } = useSessionModelsQuery(harness);
  const summary = useMemo(() => currentModelLabel(value, catalog), [value, catalog]);

  return (
    <div className="session-model-picker" data-testid="session-model-select">
      {catalogOpen ? (
        <div
          className="session-model-catalog"
          id={`${id}-catalog`}
          data-testid="session-model-catalog"
        >
          <div className="session-model-catalog-head">
            <span className="field-label" id={id}>
              Model
            </span>
            <button
              type="button"
              className="session-model-done"
              data-testid="session-model-done"
              aria-expanded
              aria-controls={`${id}-catalog`}
              onClick={() => setCatalogOpen(false)}
            >
              Done
            </button>
          </div>
          <ModelPicker
            agent={harness}
            agentLabel={agentLabel(harness)}
            value={value}
            disabled={disabled}
            onChange={onChange}
          />
        </div>
      ) : (
        <div className="session-model-summary" data-testid="session-model-summary">
          <span className="field-label" id={id}>
            Model
          </span>
          <div className="session-model-current-row">
            <span className="session-model-current" data-testid="session-model-current">
              {summary}
            </span>
            <button
              type="button"
              className="session-model-change"
              data-testid="session-model-change"
              aria-expanded={catalogOpen}
              aria-controls={`${id}-catalog`}
              disabled={disabled}
              onClick={() => setCatalogOpen(true)}
            >
              Change
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
