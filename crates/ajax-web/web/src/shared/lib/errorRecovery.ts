import { ApiError } from "./api";

export type RecoveryHint = "retry" | "open_terminal" | "reload_session" | "none";

type ErrorInput =
  | { message?: string; error?: string | null; code?: string | null; kind?: string }
  | ApiError
  | unknown;

function normalizeInput(error: ErrorInput): {
  message: string;
  code: string | null;
  kind: string | null;
} {
  if (error instanceof ApiError) {
    return { message: error.message, code: error.code, kind: error.kind };
  }
  if (typeof error === "object" && error !== null) {
    const obj = error as Record<string, unknown>;
    const message =
      (typeof obj.message === "string" && obj.message) ||
      (typeof obj.error === "string" && obj.error) ||
      "";
    const code = typeof obj.code === "string" && obj.code.length > 0 ? obj.code : null;
    const kind = typeof obj.kind === "string" ? obj.kind : null;
    return { message, code, kind };
  }
  return { message: "", code: null, kind: null };
}

function recoveryKey(code: string | null, kind: string | null): string | null {
  if (code) return code;
  if (kind === "conflict") return "conflict";
  if (kind === "stale-session") return "stale_session";
  if (kind === "network") return "network";
  if (kind === "terminal") return "needs_terminal";
  return null;
}

function appendHintSuffix(message: string, hint: RecoveryHint): string {
  if (hint === "open_terminal" && !/terminal/i.test(message)) {
    return `${message} — open the terminal`;
  }
  if (hint === "reload_session" && !/reload/i.test(message)) {
    return `${message} — reload the page`;
  }
  return message;
}

export function operatorErrorPresentation(error: ErrorInput): {
  message: string;
  hint: RecoveryHint;
  telemetryKind: string;
} {
  const { message: rawMessage, code, kind } = normalizeInput(error);
  const key = recoveryKey(code, kind);

  let message = rawMessage;
  let hint: RecoveryHint = "retry";
  let telemetryKind = "operation_failed";

  switch (key) {
    case "needs_terminal":
      message = rawMessage || "Use the terminal for this action";
      hint = "open_terminal";
      telemetryKind = "needs_terminal";
      break;
    case "stale_session":
      message = rawMessage || "Session expired — reload";
      hint = "reload_session";
      telemetryKind = "stale_session";
      break;
    case "conflict":
      message = rawMessage || "Action failed";
      hint = "retry";
      telemetryKind = "conflict";
      break;
    case "task_not_found":
      message = rawMessage || "Action failed";
      hint = "none";
      telemetryKind = "task_not_found";
      break;
    case "confirmation_required":
      message = rawMessage || "Action failed";
      hint = "retry";
      telemetryKind = "confirmation_required";
      break;
    case "unsupported_action":
    case "unknown_action":
      message = rawMessage || "Action failed";
      hint = "none";
      telemetryKind = "operation_failed";
      break;
    case "network":
      message = "Action failed — network error";
      hint = "retry";
      telemetryKind = "network";
      break;
    case "command_failed":
    default:
      message = rawMessage || "Action failed";
      hint = "retry";
      telemetryKind = "operation_failed";
      break;
  }

  return {
    message: appendHintSuffix(message, hint),
    hint,
    telemetryKind,
  };
}
