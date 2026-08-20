export const DEFAULT_SESSION_MODEL = "auto";

/**
 * A selection is the model id plus any harness options, written
 * `opus|effort=high`. The server parses the same form.
 */
export function encodeModelSelection(model: string, options: Record<string, string>): string {
  const extras = Object.entries(options)
    .filter(([key, value]) => key && value)
    .map(([key, value]) => `|${key}=${value}`)
    .join("");
  return model ? `${model}${extras}` : "";
}

export function decodeModelSelection(raw: string): {
  model: string;
  options: Record<string, string>;
} {
  const [model = "", ...rest] = raw.split("|");
  const options: Record<string, string> = {};
  for (const part of rest) {
    const [key, value] = part.split("=");
    if (key && value) options[key] = value;
  }
  return { model, options };
}
