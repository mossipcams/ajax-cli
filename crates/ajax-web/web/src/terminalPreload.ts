import { Ghostty, type Ghostty as GhosttyRuntime } from "ghostty-web";

export const GHOSTTY_WASM_URL = "/ghostty-vt.wasm";

let ghosttyRuntime: Promise<GhosttyRuntime> | undefined;

export function preloadGhosttyRuntime(): Promise<GhosttyRuntime> {
  ghosttyRuntime ??= Ghostty.load(GHOSTTY_WASM_URL);
  return ghosttyRuntime;
}

export function preloadTerminalView(): Promise<unknown> {
  return import("./components/TerminalRawView.svelte");
}

export function warmTerminalAssets(): Promise<unknown[]> {
  return Promise.all([preloadGhosttyRuntime(), preloadTerminalView()]);
}
