const ORCHESTRATION_CHAT_KEY = "ajax.web.session.orchestrationChat";

export const orchestrationChatChangedEvent = "ajax:orchestration-chat-changed";

export function readOrchestrationChatFlag(): boolean {
  try {
    return localStorage.getItem(ORCHESTRATION_CHAT_KEY) === "true";
  } catch {
    return false;
  }
}

export function writeOrchestrationChatFlag(enabled: boolean): void {
  try {
    localStorage.setItem(ORCHESTRATION_CHAT_KEY, enabled ? "true" : "false");
    window.dispatchEvent(new Event(orchestrationChatChangedEvent));
  } catch {
    // ponytail: private mode may block storage; flag stays off.
  }
}
