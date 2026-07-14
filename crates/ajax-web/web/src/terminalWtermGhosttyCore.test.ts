import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { wasmExportsInclude, loadWtermGhosttyCore } from "./terminalWtermGhosttyCore";

const repoRoot = join(import.meta.dirname, "../../../..");
const wtermWasm = readFileSync(join(repoRoot, "node_modules/@wterm/ghostty/wasm/ghostty-vt.wasm"));
const ghosttyWebWasm = readFileSync(join(repoRoot, "node_modules/ghostty-web/ghostty-vt.wasm"));


const ghosttyLoad = vi.hoisted(() => vi.fn(async () => ({ core: "ok" })));

vi.mock("@wterm/ghostty", () => ({
  GhosttyCore: {
    load: ghosttyLoad,
  },
}));

describe("terminalWtermGhosttyCore", () => {
  beforeEach(() => {
    ghosttyLoad.mockClear();
  });

  it("detects init on wterm wasm and not on ghostty-web wasm", () => {
    expect(wasmExportsInclude(wtermWasm.buffer.slice(wtermWasm.byteOffset, wtermWasm.byteOffset + wtermWasm.byteLength), "init")).toBe(
      true,
    );
    expect(
      wasmExportsInclude(
        ghosttyWebWasm.buffer.slice(ghosttyWebWasm.byteOffset, ghosttyWebWasm.byteOffset + ghosttyWebWasm.byteLength),
        "init",
      ),
    ).toBe(false);
  });

  it("rejects HTTP failures with a rebuild hint", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: false, status: 404, arrayBuffer: async () => new ArrayBuffer(0) })),
    );
    await expect(loadWtermGhosttyCore()).rejects.toThrow(/HTTP 404/);
  });

  it("rejects ghostty-web bytes served at the wterm URL", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        status: 200,
        arrayBuffer: async () =>
          ghosttyWebWasm.buffer.slice(ghosttyWebWasm.byteOffset, ghosttyWebWasm.byteOffset + ghosttyWebWasm.byteLength),
      })),
    );
    await expect(loadWtermGhosttyCore()).rejects.toThrow(/missing init/);
  });

  it("loads GhosttyCore from a blob URL after validating wterm bytes", async () => {
    const createObjectURL = vi.fn(() => "blob:wterm-ok");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { ...URL, createObjectURL, revokeObjectURL });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        status: 200,
        arrayBuffer: async () =>
          wtermWasm.buffer.slice(wtermWasm.byteOffset, wtermWasm.byteOffset + wtermWasm.byteLength),
      })),
    );

    await expect(loadWtermGhosttyCore()).resolves.toEqual({ core: "ok" });
    expect(ghosttyLoad).toHaveBeenCalledWith({ wasmPath: "blob:wterm-ok" });
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:wterm-ok");
  });
});
