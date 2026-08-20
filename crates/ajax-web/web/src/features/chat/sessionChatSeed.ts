/** Treat "within this many px of the bottom" as following the live edge. */
export const PIN_THRESHOLD_PX = 48;
export const RECONNECT_BASE_MS = 500;
export const RECONNECT_MAX_MS = 8000;
/** Handshake failures before the first ready are capped; post-ready drops retry forever. */
export const MAX_HANDSHAKE_ATTEMPTS = 5;
