const ESC = "\u001b";
const TERMINAL_DA_REPORT = new RegExp(`${ESC}\\[[?>][0-9;]*c`, "g");

export function filterTerminalInputReports(data: string): string {
  if (!data || !data.includes(ESC)) return data;
  return data.replace(TERMINAL_DA_REPORT, "");
}
