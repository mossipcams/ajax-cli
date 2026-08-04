/** Document flag so page-swipe (capture) can abort once terminal selection owns the gesture. */
const TERMINAL_SELECTING_DATASET = "ajaxTerminalSelecting";

export function setTerminalSelecting(active: boolean): void {
  if (active) {
    document.documentElement.dataset[TERMINAL_SELECTING_DATASET] = "1";
  } else {
    delete document.documentElement.dataset[TERMINAL_SELECTING_DATASET];
  }
}

export function isTerminalSelecting(): boolean {
  return document.documentElement.dataset[TERMINAL_SELECTING_DATASET] === "1";
}
