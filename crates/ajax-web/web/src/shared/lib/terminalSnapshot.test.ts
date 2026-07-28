import { describe, it, expect, vi } from "vitest";
import { createTerminalSnapshot } from "./terminalSnapshot";

describe("createTerminalSnapshot", () => {
  it("capture stores serialize output and restore writes it back", () => {
    const serialize = vi.fn().mockReturnValue("line one\nline two");
    const snapshot = createTerminalSnapshot(serialize);
    const term = {
      reset: vi.fn(),
      write: vi.fn(),
    };

    snapshot.capture();
    const restored = snapshot.restore(term);

    expect(serialize).toHaveBeenCalled();
    expect(restored).toBe(true);
    expect(term.reset).toHaveBeenCalled();
    expect(term.write).toHaveBeenCalledWith("line one\nline two");
  });

  it("clear drops the snapshot and restore is a no-op", () => {
    const snapshot = createTerminalSnapshot(() => "buffer");
    const term = {
      reset: vi.fn(),
      write: vi.fn(),
    };

    snapshot.capture();
    snapshot.clear();

    expect(snapshot.getSnapshot()).toBeUndefined();
    expect(snapshot.restore(term)).toBe(false);
    expect(term.reset).not.toHaveBeenCalled();
    expect(term.write).not.toHaveBeenCalled();
  });

  it("restore does not touch connection stubs", () => {
    const snapshot = createTerminalSnapshot(() => "data");
    const term = {
      reset: vi.fn(),
      write: vi.fn(),
    };
    const connection = {
      dispose: vi.fn(),
      send: vi.fn(),
    };

    snapshot.capture();
    snapshot.restore(term);

    expect(connection.dispose).not.toHaveBeenCalled();
    expect(connection.send).not.toHaveBeenCalled();
  });
});
