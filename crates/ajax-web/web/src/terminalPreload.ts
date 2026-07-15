import { Ghostty, type Ghostty as GhosttyRuntime } from "ghostty-web";
import { isTerminalSurfaceV2Enabled } from "./terminalSurfaceSetting";

export const GHOSTTY_WASM_URL = "/ghostty-vt.wasm";

let ghosttyRuntime: Promise<GhosttyRuntime> | undefined;

export function preloadGhosttyRuntime(): Promise<GhosttyRuntime> {
  ghosttyRuntime ??= Ghostty.load(GHOSTTY_WASM_URL);
  return ghosttyRuntime;
}

export function preloadTerminalView(): Promise<unknown> {
  return import("./components/TerminalRawView.svelte");
}

export function preloadWtermTerminalView(): Promise<unknown> {
  return import("./components/WtermTerminalView.svelte");
}

/** Warm the active surface only — never Ghostty while Surface V2 is enabled. */
export function warmTerminalAssets(): Promise<unknown[]> {
  if (isTerminalSurfaceV2Enabled()) {
    return Promise.all([preloadWtermTerminalView()]);
  }
  return Promise.all([preloadGhosttyRuntime(), preloadTerminalView()]);
}
