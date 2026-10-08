export const AGENTS = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude" },
  { value: "cursor", label: "Cursor" },
  { value: "pi", label: "Pi" },
] as const;

export function agentLabel(value: string): string {
  return AGENTS.find((option) => option.value === value)?.label ?? value;
}
