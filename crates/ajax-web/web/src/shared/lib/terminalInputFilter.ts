// Remote PTY owns device-attribute negotiation; browser xterm must not answer
// into stdin. Only strip DA replies (final byte `c` with ?/>). Do not strip
// CSI … R — that collides with modified F3 (`ESC [1;5R`).
const ESC = "\u001b";
const TERMINAL_DA_REPORT = new RegExp(`${ESC}\\[[?>][0-9;]*c`, "g");

/** Strip automatic xterm Device Attributes replies from outbound terminal input. */
export function filterTerminalInputReports(data: string): string {
  if (!data || !data.includes(ESC)) return data;
  return data.replace(TERMINAL_DA_REPORT, "");
}
