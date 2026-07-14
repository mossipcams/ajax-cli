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

type GhosttyWasmModule = {
  exports: WebAssembly.Exports;
  instance: WebAssembly.Instance;
};

type GhosttyCoreConstructable = {
  new (
    wasm: GhosttyWasmModule,
    options: { scrollbackLimit?: number; wasmPath?: string },
  ): GhosttyCore;
};

async function fetchWtermWasmBytes(): Promise<ArrayBuffer> {
  let response: Response;
  try {
    response = await fetch(WTERM_GHOSTTY_WASM_URL, { cache: "no-store" });
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

async function instantiateWtermWasm(bytes: ArrayBuffer): Promise<GhosttyWasmModule> {
  let wasmMemory: WebAssembly.Memory | undefined;
  let instance: WebAssembly.Instance;
  try {
    ({ instance } = await WebAssembly.instantiate(bytes, {
      env: {
        log(ptr: number, len: number) {
          if (!wasmMemory) return;
          const text = new TextDecoder().decode(new Uint8Array(wasmMemory.buffer, ptr, len));
          console.log("[ghostty-vt]", text);
        },
      },
    }));
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`wterm wasm instantiate failed (${detail})`);
  }
  wasmMemory = instance.exports.memory as WebAssembly.Memory;
  return { exports: instance.exports, instance };
}

/**
 * Load @wterm/ghostty's core from the Ajax-served distinct URL.
 *
 * Instantiates from fetched bytes (no Safari blob: re-fetch). Constructs
 * GhosttyCore with a real options object — `init()` reads
 * `_options.scrollbackLimit` and crashes if `_options` is undefined.
 */
export async function loadWtermGhosttyCore(): Promise<GhosttyCore> {
  const bytes = await fetchWtermWasmBytes();
  const wasm = await instantiateWtermWasm(bytes);
  const options = { scrollbackLimit: terminalScrollbackLines() };
  // Runtime constructor is public in JS; .d.ts marks it private.
  const Core = GhosttyCore as unknown as GhosttyCoreConstructable;
  return new Core(wasm, options);
}

/**
 * Prove the constructed core can init/write — used by integration tests and
 * as a post-load sanity check before handing the core to WTerm.
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
