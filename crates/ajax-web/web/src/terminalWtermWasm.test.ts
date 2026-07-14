import { describe, it, expect } from "vitest";
import { WTERM_GHOSTTY_WASM_URL } from "./terminalWtermWasm";

describe("terminalWtermWasm", () => {
  it("uses a path that does not collide with ghostty-web", () => {
    expect(WTERM_GHOSTTY_WASM_URL).toBe("/wterm-ghostty-vt.wasm");
    expect(WTERM_GHOSTTY_WASM_URL).not.toBe("/ghostty-vt.wasm");
  });
});
