// Remote PTY owns device-attribute negotiation; browser xterm must not answer
// into stdin. Only strip DA replies (final byte `c` with ?/>). Do not strip
// CSI … R — that collides with modified F3 (`\x1b[1;5R`).
const TERMINAL_DA_REPORT = /\x1b\[[?>][0-9;]*c/g;

/** Strip automatic xterm Device Attributes replies from outbound terminal input. */
export function filterTerminalInputReports(data: string): string {
  if (!data || !data.includes("\x1b")) return data;
  return data.replace(TERMINAL_DA_REPORT, "");
}
