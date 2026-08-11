import { useEffect, useState } from "react";
import {
  DEFAULT_SESSION_MODEL,
  fetchSessionModels,
  type SessionModelOption,
} from "./sessionModel";

interface Props {
  id: string;
  value: string;
  disabled?: boolean;
  onChange: (model: string) => void;
}

export default function SessionModelSelect({ id, value, disabled, onChange }: Props) {
  const [options, setOptions] = useState<SessionModelOption[]>([
    { id: DEFAULT_SESSION_MODEL, label: "Auto" },
  ]);

  useEffect(() => {
    let cancelled = false;
    void fetchSessionModels().then((models) => {
      if (!cancelled) setOptions(models);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const known = options.some((option) => option.id === value);
  const selectValue = known ? value : value || DEFAULT_SESSION_MODEL;

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
