/** True when a host `error` event should revert an optimistic in-session model change. */
export function isSessionModelChangeFailure(message: string): boolean {
  const normalized = message.trim().toLowerCase();
  if (!normalized) return false;
  if (normalized.includes("session model")) return true;
  if (normalized.includes("could not be verified")) return true;
  if (normalized.includes("was refused")) return true;
  if (normalized.includes("unsupported model")) return true;
  if (normalized.includes("model id must not contain whitespace")) return true;
  if (normalized.includes("registry write failed")) return true;
  if (normalized.includes("cockpit state changed while updating session model")) return true;
  if (normalized.includes("agent has no acp entry point")) return true;
  if (normalized.includes("no acp mapping")) return true;
  return false;
}
