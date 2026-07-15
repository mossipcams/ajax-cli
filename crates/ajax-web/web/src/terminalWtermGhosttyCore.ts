import { GhosttyCore } from "@wterm/ghostty";
import { terminalScrollbackLines } from "./terminalGeometry";
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

async function fetchWtermWasmBytes(): Promise<ArrayBuffer> {
  let response: Response;
  try {
    response = await fetch(WTERM_GHOSTTY_WASM_URL);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(
      `wterm wasm fetch failed (${detail}) at ${WTERM_GHOSTTY_WASM_URL} — rebuild/restart ajax and hard-refresh`,
    );
  }
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
  return bytes;
}

let pendingWtermGhosttyCore: Promise<GhosttyCore> | undefined;

async function createWtermGhosttyCore(): Promise<GhosttyCore> {
  await fetchWtermWasmBytes();
  try {
    return await GhosttyCore.load({
      wasmPath: WTERM_GHOSTTY_WASM_URL,
      scrollbackLimit: terminalScrollbackLines(),
    });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`GhosttyCore.load failed (${detail}) for ${WTERM_GHOSTTY_WASM_URL}`);
  }
}

/** Begin loading the next unconsumed @wterm/ghostty core; repeated calls share one load. */
export function preloadWtermGhosttyCore(): Promise<GhosttyCore> {
  pendingWtermGhosttyCore ??= createWtermGhosttyCore();
  return pendingWtermGhosttyCore;
}

/**
 * Load @wterm/ghostty via the official API after validating Ajax's distinct URL.
 *
 * Uses `GhosttyCore.load({ wasmPath, scrollbackLimit })` so `_options` is always
 * the real options object (private-constructor mistakes caused Safari yellow
 * banners). Validates bytes first so we never accept ghostty-web's binary.
 * Second fetch is intentional and must stay on the HTTP URL (not blob:).
 */
export async function loadWtermGhosttyCore(): Promise<GhosttyCore> {
  const pending = pendingWtermGhosttyCore;
  pendingWtermGhosttyCore = undefined;
  return pending ?? createWtermGhosttyCore();
}

/**
 * Integration-test helper: prove init/write after load.
 * Not used in production mount (avoid double-init before WTerm).
 */
export function smokeInitWtermGhosttyCore(core: GhosttyCore): void {
  core.init(40, 10);
  core.writeString("Ajax wterm smoke\r\n");
  const cell = core.getCell(0, 0);
  if (cell.char !== "A".charCodeAt(0)) {
    throw new Error(
      `wterm smoke init wrote unexpected cell char=${cell.char} (expected 'A')`,
    );
  }
}
