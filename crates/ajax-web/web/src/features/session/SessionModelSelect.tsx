import { useMemo } from "react";
import {
  DEFAULT_SESSION_MODEL,
  type SessionModelCatalog,
} from "./sessionModel";
import { useSessionModelsQuery } from "./useSessionModelsQuery";

interface Props {
  id: string;
  /** Empty means "let the server pick" — shown as the catalog default. */
  value: string;
  disabled?: boolean;
  /** Harness whose catalog to list. */
  agent?: string;
  onChange: (model: string) => void;
}

export default function SessionModelSelect({ id, value, disabled, agent, onChange }: Props) {
  const fallbackCatalog = useMemo<SessionModelCatalog>(
    () => ({
      models: [{ id: DEFAULT_SESSION_MODEL, label: "Auto" }],
      default: DEFAULT_SESSION_MODEL,
    }),
    [],
  );
  const { data: catalog = fallbackCatalog } = useSessionModelsQuery(agent ?? "cursor");

  const options = catalog.models;
  const known = options.some((option) => option.id === value);
  const selectValue = known ? value : value || catalog.default;

  return (
    <label className="session-model-picker" htmlFor={id}>
      <span className="session-model-picker-label">Model</span>
      <select
        id={id}
        data-testid="session-model-select"
        value={selectValue}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
      >
        {!known && value ? <option value={value}>{value}</option> : null}
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}
