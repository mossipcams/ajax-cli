import { GhosttyCore } from "@wterm/ghostty";
import { WTERM_GHOSTTY_WASM_URL } from "./terminalWtermWasm";

/** Minimal WASM export-section scan — avoids trusting HTTP path alone. */
export function wasmExportsInclude(bytes: ArrayBuffer, name: string): boolean {
  const u8 = new Uint8Array(bytes);
  if (u8.length < 8 || u8[0] !== 0x00 || u8[1] !== 0x61 || u8[2] !== 0x73 || u8[3] !== 0x6d) {
    return false;
  }
  let i = 8;
  while (i < u8.length) {
    const id = u8[i++];
    let size = 0;
    let shift = 0;
    let b = 0;
    do {
      b = u8[i++];
      size |= (b & 0x7f) << shift;
      shift += 7;
    } while (b & 0x80);
    const start = i;
    if (id === 7) {
      let j = start;
      let count = 0;
      shift = 0;
      do {
        b = u8[j++];
        count |= (b & 0x7f) << shift;
        shift += 7;
      } while (b & 0x80);
      for (let n = 0; n < count; n++) {
        let len = 0;
        shift = 0;
        do {
          b = u8[j++];
          len |= (b & 0x7f) << shift;
          shift += 7;
        } while (b & 0x80);
        const exportName = new TextDecoder().decode(u8.subarray(j, j + len));
        j += len;
        j += 1; // kind
        shift = 0;
        do {
          b = u8[j++];
          shift += 7;
        } while (b & 0x80);
        if (exportName === name) return true;
      }
    }
    i = start + size;
  }
  return false;
}

/**
 * Load @wterm/ghostty's core from the Ajax-served distinct URL.
 *
 * Never call bare `GhosttyCore.load()` — Vite rewrites that package's default
 * asset to `/ghostty-vt.wasm` (ghostty-web), which lacks `init`.
 */
export async function loadWtermGhosttyCore(): Promise<GhosttyCore> {
  const response = await fetch(WTERM_GHOSTTY_WASM_URL, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(
      `wterm wasm HTTP ${response.status} at ${WTERM_GHOSTTY_WASM_URL} — rebuild/restart ajax so that asset is embedded`,
    );
  }
  const bytes = await response.arrayBuffer();
  if (!wasmExportsInclude(bytes, "init")) {
    throw new Error(
      `wterm wasm at ${WTERM_GHOSTTY_WASM_URL} is missing init() (wrong/stale binary). Hard-refresh or rebuild ajax.`,
    );
  }
  const blobUrl = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: "application/wasm" }));
  try {
    return await GhosttyCore.load({ wasmPath: blobUrl });
  } finally {
    URL.revokeObjectURL(blobUrl);
  }
}
