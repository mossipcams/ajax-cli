export type ElicitationFieldKind = "string" | "enum" | "boolean" | "number";

export interface ElicitationEnumOption {
  value: string;
  title: string;
  description?: string;
}

export interface ElicitationFormField {
  name: string;
  kind: ElicitationFieldKind;
  title: string;
  description?: string;
  required: boolean;
  defaultValue?: string | number | boolean | string[];
  enumOptions?: ElicitationEnumOption[];
  minimum?: number;
  maximum?: number;
}

export interface PendingElicitationWire {
  requestId: string;
  message: string;
  schema: unknown;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function readString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function parseEnumOptions(property: Record<string, unknown>): ElicitationEnumOption[] | undefined {
  const oneOf = property.oneOf;
  if (Array.isArray(oneOf)) {
    const options = oneOf
      .map((entry) => {
        const record = asRecord(entry);
        if (!record) return null;
        const value = readString(record.const);
        const title = readString(record.title) ?? value;
        if (!value || !title) return null;
        return {
          value,
          title,
          ...(readString(record.description) ? { description: readString(record.description) } : {}),
        };
      })
      .filter((entry): entry is ElicitationEnumOption => entry !== null);
    return options.length ? options : undefined;
  }
  const enumValues = property.enum;
  if (Array.isArray(enumValues)) {
    const options = enumValues
      .filter((entry): entry is string => typeof entry === "string")
      .map((value) => ({ value, title: value }));
    return options.length ? options : undefined;
  }
  return undefined;
}

function parseField(name: string, property: Record<string, unknown>, required: boolean): ElicitationFormField | null {
  const type = readString(property.type);
  const title = readString(property.title) ?? name;
  const description = readString(property.description);
  const enumOptions = parseEnumOptions(property);
  if (type === "string" && enumOptions) {
    return {
      name,
      kind: "enum",
      title,
      ...(description ? { description } : {}),
      required,
      ...(readString(property.default) ? { defaultValue: readString(property.default) } : {}),
      enumOptions,
    };
  }
  if (type === "string") {
    return {
      name,
      kind: "string",
      title,
      ...(description ? { description } : {}),
      required,
      ...(readString(property.default) ? { defaultValue: readString(property.default) } : {}),
    };
  }
  if (type === "boolean") {
    return {
      name,
      kind: "boolean",
      title,
      ...(description ? { description } : {}),
      required,
      ...(typeof property.default === "boolean" ? { defaultValue: property.default } : {}),
    };
  }
  if (type === "number" || type === "integer") {
    return {
      name,
      kind: "number",
      title,
      ...(description ? { description } : {}),
      required,
      ...(typeof property.default === "number" ? { defaultValue: property.default } : {}),
      ...(typeof property.minimum === "number" ? { minimum: property.minimum } : {}),
      ...(typeof property.maximum === "number" ? { maximum: property.maximum } : {}),
    };
  }
  return null;
}

/** Parse an ACP form schema into supported operator fields. Unsupported properties are skipped. */
export function parseElicitationFormSchema(schema: unknown): ElicitationFormField[] {
  const root = asRecord(schema);
  if (!root) return [];
  const properties = asRecord(root.properties);
  if (!properties) return [];
  const required = Array.isArray(root.required)
    ? root.required.filter((entry): entry is string => typeof entry === "string")
    : [];
  return Object.entries(properties)
    .map(([name, property]) => parseField(name, asRecord(property) ?? {}, required.includes(name)))
    .filter((field): field is ElicitationFormField => field !== null);
}

export function parsePendingElicitationWire(value: unknown): PendingElicitationWire | null {
  const record = asRecord(value);
  if (!record || typeof record.requestId !== "string" || typeof record.message !== "string") {
    return null;
  }
  return {
    requestId: record.requestId,
    message: record.message,
    schema: record.schema,
  };
}

export function buildElicitationContent(
  fields: ElicitationFormField[],
  values: Record<string, string | number | boolean | string[]>,
): Record<string, string | number | boolean | string[]> {
  const content: Record<string, string | number | boolean | string[]> = {};
  for (const field of fields) {
    const value = values[field.name];
    if (value === undefined || value === "") continue;
    content[field.name] = value;
  }
  return content;
}

function fieldHasValue(
  field: ElicitationFormField,
  value: string | number | boolean | string[] | undefined,
): boolean {
  if (value === undefined) return false;
  if (field.kind === "boolean") return true;
  if (field.kind === "number") return Number.isFinite(value);
  if (typeof value === "string") return value.trim().length > 0;
  if (Array.isArray(value)) return value.length > 0;
  return true;
}

/** Required fields must have values before Accept can dispatch content. */
export function isElicitationValid(
  fields: ElicitationFormField[],
  values: Record<string, string | number | boolean | string[]>,
): boolean {
  return fields.every((field) => !field.required || fieldHasValue(field, values[field.name]));
}
