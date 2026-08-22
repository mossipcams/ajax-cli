import { useMemo, useState } from "react";
import { Button } from "@/shared/ui/button";
import {
  buildElicitationContent,
  isElicitationValid,
  parseElicitationFormSchema,
  type ElicitationFormField,
} from "@/shared/lib/liveSessionElicitation";
import type { ElicitationDecision } from "../session/public";

interface Props {
  decision: ElicitationDecision;
  connected: boolean;
  onAccept: (content: Record<string, string | number | boolean | string[]>) => void;
  onDecline: () => void;
  onCancel: () => void;
}

function initialValues(fields: ElicitationFormField[]): Record<string, string | number | boolean> {
  const values: Record<string, string | number | boolean> = {};
  for (const field of fields) {
    if (field.defaultValue !== undefined && !Array.isArray(field.defaultValue)) {
      values[field.name] = field.defaultValue;
    } else if (field.kind === "boolean") {
      values[field.name] = false;
    } else {
      values[field.name] = "";
    }
  }
  return values;
}

function FieldInput({
  field,
  value,
  onChange,
}: {
  field: ElicitationFormField;
  value: string | number | boolean;
  onChange: (next: string | number | boolean) => void;
}) {
  const id = `elicitation-${field.name}`;
  if (field.kind === "boolean") {
    return (
      <label className="session-elicitation-check" htmlFor={id}>
        <input
          id={id}
          type="checkbox"
          checked={Boolean(value)}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span>{field.title}</span>
      </label>
    );
  }
  if (field.kind === "enum" && field.enumOptions?.length) {
    return (
      <label className="session-elicitation-field" htmlFor={id}>
        <span className="session-elicitation-label">{field.title}</span>
        <select
          id={id}
          value={String(value)}
          required={field.required}
          onChange={(event) => onChange(event.target.value)}
        >
          {!field.required ? <option value="">Select…</option> : null}
          {field.enumOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.title}
            </option>
          ))}
        </select>
      </label>
    );
  }
  if (field.kind === "number") {
    return (
      <label className="session-elicitation-field" htmlFor={id}>
        <span className="session-elicitation-label">{field.title}</span>
        <input
          id={id}
          type="number"
          value={String(value)}
          required={field.required}
          min={field.minimum}
          max={field.maximum}
          onChange={(event) => onChange(Number(event.target.value))}
        />
      </label>
    );
  }
  return (
    <label className="session-elicitation-field" htmlFor={id}>
      <span className="session-elicitation-label">{field.title}</span>
      <input
        id={id}
        type="text"
        value={String(value)}
        required={field.required}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

export default function ElicitationPanel({
  decision,
  connected,
  onAccept,
  onDecline,
  onCancel,
}: Props) {
  const fields = useMemo(() => parseElicitationFormSchema(decision.schema), [decision.schema]);
  const [values, setValues] = useState(() => initialValues(fields));
  const acceptEnabled = connected && isElicitationValid(fields, values);

  return (
    <div
      className="session-elicitation"
      data-testid="session-elicitation"
      role="form"
      aria-label="Agent request"
    >
      <p className="session-elicitation-message">{decision.message}</p>
      <div className="session-elicitation-fields">
        {fields.map((field) => (
          <FieldInput
            key={field.name}
            field={field}
            value={values[field.name] ?? ""}
            onChange={(next) => setValues((current) => ({ ...current, [field.name]: next }))}
          />
        ))}
      </div>
      <div className="session-elicitation-actions">
        <Button
          type="button"
          variant="default"
          disabled={!acceptEnabled}
          onClick={() => {
            if (!isElicitationValid(fields, values)) return;
            onAccept(buildElicitationContent(fields, values));
          }}
        >
          Accept
        </Button>
        <Button type="button" variant="secondary" disabled={!connected} onClick={onDecline}>
          Decline
        </Button>
        <Button type="button" variant="secondary" disabled={!connected} onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
