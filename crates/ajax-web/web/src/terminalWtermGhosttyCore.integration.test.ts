/**
 * Unmocked Ghostty/wterm contract tests.
 *
 * These load the real @wterm/ghostty WASM from node_modules. They exist because
 * mocked unit tests previously shipped constructor/init bugs that only failed
 * on device (yellow Surface V2 banner).
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { WTerm } from "@wterm/dom";
import {
  loadWtermGhosttyCore,
  smokeInitWtermGhosttyCore,
} from "./terminalWtermGhosttyCore";
import { WTERM_GHOSTTY_WASM_URL } from "./terminalWtermWasm";

const repoRoot = join(import.meta.dirname, "../../../..");
const wtermWasmPath = join(repoRoot, "node_modules/@wterm/ghostty/wasm/ghostty-vt.wasm");
const ghosttyWebWasmPath = join(repoRoot, "node_modules/ghostty-web/ghostty-vt.wasm");

function wasmArrayBuffer(path: string): ArrayBuffer {
  const buf = readFileSync(path);
  return buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
}

describe("terminalWtermGhosttyCore integration (real WASM)", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        expect(url).toContain(WTERM_GHOSTTY_WASM_URL);
        return {
          ok: true,
          status: 200,
          arrayBuffer: async () => wasmArrayBuffer(wtermWasmPath),
        };
      }),
    );
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
        unobserve() {}
      },
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    document.body.innerHTML = "";
  });

  it("constructs GhosttyCore that can init and write cells", async () => {
    const core = await loadWtermGhosttyCore();
    expect(() => smokeInitWtermGhosttyCore(core)).not.toThrow();
    expect(core.getCols()).toBe(40);
    expect(core.getRows()).toBe(10);
    expect(core.getCell(0, 0).char).toBe("A".charCodeAt(0));
  });

  it("rejects the ghostty-web binary even if served at the wterm URL", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        status: 200,
        arrayBuffer: async () => wasmArrayBuffer(ghosttyWebWasmPath),
      })),
    );
    await expect(loadWtermGhosttyCore()).rejects.toThrow(/missing init/);
  });

  it("WTerm.init + write paints text with the loaded core", async () => {
    const core = await loadWtermGhosttyCore();
    const host = document.createElement("div");
    host.style.width = "320px";
    host.style.height = "170px";
    // wterm measures char size from a probe in the host; give non-zero boxes.
    Object.defineProperty(host, "clientWidth", { configurable: true, value: 320 });
    Object.defineProperty(host, "clientHeight", { configurable: true, value: 170 });
    document.body.appendChild(host);

    const term = new WTerm(host, {
      core,
      cols: 40,
      rows: 10,
      autoResize: false,
    });
    await term.init();
    term.write("Hello wterm\r\n");

    // Allow the scheduled render (setTimeout 0 + rAF).
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

    const text = host.textContent ?? "";
    expect(text).toContain("Hello wterm");
    term.destroy();
  });
});
