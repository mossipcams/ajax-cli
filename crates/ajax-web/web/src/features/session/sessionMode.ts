import { useEffect, useState } from "react";

export const ORCHESTRATION_CHAT_STORAGE_KEY = "ajax.web.session.orchestrationChat";

const SESSION_MODE_EVENT = "ajax:session-mode";

export function readOrchestrationChatEnabled(): boolean {
  try {
    return localStorage.getItem(ORCHESTRATION_CHAT_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

export function writeOrchestrationChatEnabled(enabled: boolean): void {
  try {
    localStorage.setItem(ORCHESTRATION_CHAT_STORAGE_KEY, enabled ? "true" : "false");
    window.dispatchEvent(new CustomEvent(SESSION_MODE_EVENT));
  } catch {
    // Private mode / storage denied: preference just won't stick.
  }
}

export function subscribeOrchestrationChat(listener: () => void): () => void {
  const onStorage = (event: StorageEvent) => {
    if (event.key === ORCHESTRATION_CHAT_STORAGE_KEY) listener();
  };
  const onCustom = () => listener();
  window.addEventListener("storage", onStorage);
  window.addEventListener(SESSION_MODE_EVENT, onCustom);
  return () => {
    window.removeEventListener("storage", onStorage);
    window.removeEventListener(SESSION_MODE_EVENT, onCustom);
  };
}

export function useOrchestrationChatEnabled(): boolean {
  const [enabled, setEnabled] = useState(readOrchestrationChatEnabled);
  useEffect(
    () => subscribeOrchestrationChat(() => setEnabled(readOrchestrationChatEnabled())),
    [],
  );
  return enabled;
}
