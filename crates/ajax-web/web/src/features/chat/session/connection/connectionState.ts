/** Explicit reconnect lifecycle for one mounted task session. */
export type ConnectionState =
  | "connecting"
  | "connected"
  | "waiting"
  | "failed"
  | "disposed";

export function initialConnectionState(): ConnectionState {
  return "connecting";
}

export function connectionStateIsActive(state: ConnectionState): boolean {
  return state === "connecting" || state === "connected" || state === "waiting";
}

export function connectionStateAllowsSend(state: ConnectionState): boolean {
  return state === "connected";
}

const ALL_CONNECTION_STATES: ConnectionState[] = [
  "connecting",
  "connected",
  "waiting",
  "failed",
  "disposed",
];

/** ponytail: compile-time exhaustiveness guard for ConnectionState switches. */
export function assertConnectionState(value: ConnectionState): ConnectionState {
  if (!ALL_CONNECTION_STATES.includes(value)) {
    throw new Error(`Unknown connection state: ${String(value)}`);
  }
  return value;
}
