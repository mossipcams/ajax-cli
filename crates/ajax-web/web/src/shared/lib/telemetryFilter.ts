export type TelemetryPropValue = string | number | boolean | null | undefined;

export type TelemetryProps = Record<string, TelemetryPropValue>;

const SENSITIVE_KEY_FRAGMENTS = [
  "terminal",
  "pty",
  "prompt",
  "token",
  "secret",
  "password",
  "passwd",
  "credential",
  "bearer",
  "apikey",
  "api_key",
  "authorization",
  "command",
  "cmd",
  "shell",
  "stdin",
  "stdout",
  "stderr",
  "buffer",
  "xterm",
  "source",
  "snippet",
  "diff",
  "patch",
  "output",
  "content",
  "message",
] as const;

const TOKEN_VALUE_RE =
  /^(?:eyJ[A-Za-z0-9_-]+\.eyJ|phc_[A-Za-z0-9]+|sk-[A-Za-z0-9]+|ghp_[A-Za-z0-9]+)/;
const SHELL_COMMAND_RE =
  /^(?:sudo |cd |rm |git |curl |npm |cargo |bash |sh |zsh )/;
const SOURCE_CODE_RE = /(?:^|\n)\s*(?:import |export |function |const |let |class )/;

function isSensitiveKey(key: string): boolean {
  const lower = key.toLowerCase();
  return SENSITIVE_KEY_FRAGMENTS.some((fragment) => lower.includes(fragment));
}

function isSensitiveString(value: string): boolean {
  if (TOKEN_VALUE_RE.test(value)) {
    return true;
  }
  if (value.length > 500) {
    return true;
  }
  if (value.includes("\n") && value.length > 40) {
    return true;
  }
  if (SHELL_COMMAND_RE.test(value)) {
    return true;
  }
  if (SOURCE_CODE_RE.test(value)) {
    return true;
  }
  return false;
}

function isSensitiveValue(value: TelemetryPropValue): boolean {
  if (typeof value === "string") {
    return isSensitiveString(value);
  }
  return false;
}

/** Drop sensitive keys and suspicious string values before PostHog capture. */
export function sanitizeTelemetryProps(
  props: TelemetryProps,
): Record<string, string | number | boolean> {
  const result: Record<string, string | number | boolean> = {};
  for (const [key, value] of Object.entries(props)) {
    if (value === null || value === undefined) {
      continue;
    }
    if (isSensitiveKey(key) || isSensitiveValue(value)) {
      continue;
    }
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      result[key] = value;
    }
  }
  return result;
}
