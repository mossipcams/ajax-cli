import type { WebSessionSymbolContext } from "@/shared/lib/types";

export type { WebSessionSymbolContext, WebSessionSymbolKind } from "@/shared/lib/types";

export const WEB_SESSION_PROTOCOL_VERSION = 2;

export type WebSessionConnectionStatus =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error"
  | "closed";

export type WebSessionRunStatus = "running" | "waiting";

export type WebSessionRole = "user" | "assistant";

export type SessionAttentionKind = "permission" | "question" | "failed" | "review";

export interface SessionAttentionItem {
  handle: string;
  requestId: string;
  kind: SessionAttentionKind;
  title: string;
  summary: string;
  options?: string[];
}

export type SessionAttentionResponse =
  | { type: "permission"; outcome: "allow-once" | "reject" }
  | { type: "question"; text: string }
  | { type: "failed"; action: "stop" | "retry" }
  | { type: "review"; action: "open" };

export interface WebSessionMessage {
  id: string;
  role: WebSessionRole;
  text: string;
  streaming?: boolean;
}

export type SessionComposerMode = "hidden" | "redirect" | "question";

export type SessionCardKind = "progress" | "operator" | "decision";

export interface SessionProgressCard {
  id: string;
  kind: "progress";
  text: string;
  streaming?: boolean;
}

export interface SessionOperatorCard {
  id: string;
  kind: "operator";
  text: string;
}

export interface SessionDecisionCard {
  id: string;
  kind: "decision";
  attention: SessionAttentionItem;
}

export type SessionCard = SessionProgressCard | SessionOperatorCard | SessionDecisionCard;

export function symbolContextChipLabel(symbol: WebSessionSymbolContext): string {
  if (symbol.kind === "method") {
    return `${symbol.name}()`;
  }
  if (symbol.kind === "function") {
    return `${symbol.name}()`;
  }
  return symbol.name;
}
