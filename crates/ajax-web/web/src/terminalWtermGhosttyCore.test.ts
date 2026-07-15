import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  wasmExportsInclude,
  loadWtermGhosttyCore,
  preloadWtermGhosttyCore,
} from "./terminalWtermGhosttyCore";

const repoRoot = join(import.meta.dirname, "../../../..");
const wtermWasm = readFileSync(join(repoRoot, "node_modules/@wterm/ghostty/wasm/ghostty-vt.wasm"));
const ghosttyWebWasm = readFileSync(join(repoRoot, "node_modules/ghostty-web/ghostty-vt.wasm"));

const ghosttyLoad = vi.hoisted(() =>
  vi.fn(async (options: { wasmPath?: string; scrollbackLimit?: number }) => ({
    options,
    runtime: "ghostty-core",
  })),
);

vi.mock("@wterm/ghostty", () => ({
  GhosttyCore: {
    load: ghosttyLoad,
  },
}));

vi.mock("./terminalGeometry", () => ({
  terminalScrollbackLines: () => 2000,
}));

describe("terminalWtermGhosttyCore (unit)", () => {
  beforeEach(() => {
    ghosttyLoad.mockClear();
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

  it("calls GhosttyCore.load with distinct wasm path and scrollbackLimit", async () => {
    const fetchMock = vi.fn(async () => ({
      ok: true,
      status: 200,
      arrayBuffer: async () =>
        wtermWasm.buffer.slice(wtermWasm.byteOffset, wtermWasm.byteOffset + wtermWasm.byteLength),
    }));
    vi.stubGlobal("fetch", fetchMock);

    const core = await loadWtermGhosttyCore();
    expect(ghosttyLoad).toHaveBeenCalledWith({
      wasmPath: "/wterm-ghostty-vt.wasm",
      scrollbackLimit: 2000,
    });
    expect(core).toMatchObject({
      options: { wasmPath: "/wterm-ghostty-vt.wasm", scrollbackLimit: 2000 },
    });
  });

  it("prewarm shares one GhosttyCore.load and consume returns a fresh core", async () => {
    const fetchMock = vi.fn(async () => ({
      ok: true,
      status: 200,
      arrayBuffer: async () =>
        wtermWasm.buffer.slice(wtermWasm.byteOffset, wtermWasm.byteOffset + wtermWasm.byteLength),
    }));
    vi.stubGlobal("fetch", fetchMock);

    const [firstPreload, secondPreload] = await Promise.all([
      preloadWtermGhosttyCore(),
      preloadWtermGhosttyCore(),
    ]);
    expect(firstPreload).toBe(secondPreload);
    expect(ghosttyLoad).toHaveBeenCalledTimes(1);

    const consumed = await loadWtermGhosttyCore();
    expect(consumed).toBe(firstPreload);
    expect(ghosttyLoad).toHaveBeenCalledTimes(1);

    const fresh = await loadWtermGhosttyCore();
    expect(fresh).not.toBe(firstPreload);
    expect(ghosttyLoad).toHaveBeenCalledTimes(2);
  });

  it("validate fetch allows HTTP cache (not no-store)", async () => {
    const fetchMock = vi.fn(async () => ({
      ok: true,
      status: 200,
      arrayBuffer: async () =>
        wtermWasm.buffer.slice(wtermWasm.byteOffset, wtermWasm.byteOffset + wtermWasm.byteLength),
    }));
    vi.stubGlobal("fetch", fetchMock);

    await loadWtermGhosttyCore();

    expect(fetchMock).toHaveBeenCalled();
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit | undefined];
    expect(init?.cache).not.toBe("no-store");
  });
});
