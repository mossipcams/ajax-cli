import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { wasmExportsInclude, loadWtermGhosttyCore } from "./terminalWtermGhosttyCore";

const repoRoot = join(import.meta.dirname, "../../../..");
const wtermWasm = readFileSync(join(repoRoot, "node_modules/@wterm/ghostty/wasm/ghostty-vt.wasm"));
const ghosttyWebWasm = readFileSync(join(repoRoot, "node_modules/ghostty-web/ghostty-vt.wasm"));

const ghosttyCoreCtor = vi.hoisted(() =>
  vi.fn(function MockCore(
    this: { wasm: unknown; options: unknown },
    wasm: unknown,
    options: unknown,
  ) {
    this.wasm = wasm;
    this.options = options;
  }),
);

vi.mock("@wterm/ghostty", () => ({
  GhosttyCore: ghosttyCoreCtor,
}));

vi.mock("./terminalGeometry", () => ({
  terminalScrollbackLines: () => 2000,
}));

describe("terminalWtermGhosttyCore (unit)", () => {
  beforeEach(() => {
    ghosttyCoreCtor.mockClear();
  });

  it("detects init on wterm wasm and not on ghostty-web wasm", () => {
    expect(
      wasmExportsInclude(
        wtermWasm.buffer.slice(wtermWasm.byteOffset, wtermWasm.byteOffset + wtermWasm.byteLength),
        "init",
      ),
    ).toBe(true);
    expect(
      wasmExportsInclude(
        ghosttyWebWasm.buffer.slice(
          ghosttyWebWasm.byteOffset,
          ghosttyWebWasm.byteOffset + ghosttyWebWasm.byteLength,
        ),
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

  it("wraps fetch network failures with path context", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new TypeError("Load failed");
      }),
    );
    await expect(loadWtermGhosttyCore()).rejects.toThrow(/wterm wasm fetch failed \(Load failed\)/);
  });

  it("rejects ghostty-web bytes served at the wterm URL", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        status: 200,
        arrayBuffer: async () =>
          ghosttyWebWasm.buffer.slice(
            ghosttyWebWasm.byteOffset,
            ghosttyWebWasm.byteOffset + ghosttyWebWasm.byteLength,
          ),
      })),
    );
    await expect(loadWtermGhosttyCore()).rejects.toThrow(/missing init/);
  });

  it("constructs GhosttyCore with wasm + scrollback options (never undefined options)", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({
        ok: true,
        status: 200,
        arrayBuffer: async () =>
          wtermWasm.buffer.slice(wtermWasm.byteOffset, wtermWasm.byteOffset + wtermWasm.byteLength),
      })),
    );

    // Real instantiate still runs; only GhosttyCore construction is mocked.
    const core = await loadWtermGhosttyCore();
    expect(ghosttyCoreCtor).toHaveBeenCalledTimes(1);
    expect(ghosttyCoreCtor).toHaveBeenCalledWith(
      expect.objectContaining({ instance: expect.anything(), exports: expect.anything() }),
      { scrollbackLimit: 2000 },
    );
    expect(core).toMatchObject({
      options: { scrollbackLimit: 2000 },
    });
  });
});
