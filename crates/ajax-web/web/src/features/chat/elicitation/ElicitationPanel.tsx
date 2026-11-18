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
  const descriptionId = `${id}-description`;
  const description =
    field.description && field.description !== field.title ? (
      <p className="session-elicitation-description" id={descriptionId}>
        {field.description}
      </p>
    ) : null;
  const describedBy = description ? descriptionId : undefined;
  const label = (
    <label className="session-elicitation-label" htmlFor={id}>
      {field.title}
    </label>
  );
  if (field.kind === "boolean") {
    return (
      <div className="session-elicitation-field">
        <label className="session-elicitation-check" htmlFor={id}>
          <input
            id={id}
            type="checkbox"
            checked={Boolean(value)}
            aria-describedby={describedBy}
            onChange={(event) => onChange(event.target.checked)}
          />
          <span>{field.title}</span>
        </label>
        {description}
      </div>
    );
  }
  if (field.kind === "enum" && field.enumOptions?.length) {
    const selected = field.enumOptions.find((option) => option.value === String(value));
    const optionDescriptionId = `${id}-option-description`;
    const optionDescribedBy =
      [describedBy, selected?.description ? optionDescriptionId : undefined].filter(Boolean).join(" ") ||
      undefined;
    return (
      <div className="session-elicitation-field">
        {label}
        {description}
        <select
          id={id}
          value={String(value)}
          required={field.required}
          aria-describedby={optionDescribedBy}
          onChange={(event) => onChange(event.target.value)}
        >
          {!field.required ? <option value="">Select…</option> : null}
          {field.required && field.defaultValue === undefined ? (
            <option value="" disabled>
              Select…
            </option>
          ) : null}
          {field.enumOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.title}
            </option>
          ))}
        </select>
        {selected?.description ? (
          <p className="session-elicitation-description" id={optionDescriptionId}>
            {selected.description}
          </p>
        ) : null}
      </div>
    );
  }
  if (field.kind === "number") {
    return (
      <div className="session-elicitation-field">
        {label}
        {description}
        <input
          id={id}
          type="number"
          value={String(value)}
          required={field.required}
          min={field.minimum}
          max={field.maximum}
          aria-describedby={describedBy}
          onChange={(event) => onChange(Number(event.target.value))}
        />
      </div>
    );
  }
  return (
    <div className="session-elicitation-field">
      {label}
      {description}
      <input
        id={id}
        type="text"
        value={String(value)}
        required={field.required}
        aria-describedby={describedBy}
        onChange={(event) => onChange(event.target.value)}
      />
    </div>
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
