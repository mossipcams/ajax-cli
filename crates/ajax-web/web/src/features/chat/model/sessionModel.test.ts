import { describe, it, expect } from "vitest";
import { isSessionModelChangeFailure, isSessionConfigChangeFailure } from "./errors";

describe("sessionModel errors", () => {
  it("recognizes host errors that should revert an in-session model change (#942)", () => {
    expect(isSessionModelChangeFailure("session model change needs a task Ajax started over ACP")).toBe(
      true,
    );
    expect(isSessionModelChangeFailure("session model composer-2.5 was refused — model refused")).toBe(
      true,
    );
    expect(
      isSessionModelChangeFailure(
        "session model composer-2.5 could not be verified — harness did not report an applied model",
      ),
    ).toBe(true);
    expect(isSessionModelChangeFailure("unsupported model")).toBe(true);
    expect(isSessionModelChangeFailure("ACP process exited")).toBe(false);
    expect(isSessionModelChangeFailure("queued prompt failed: prompt already in flight")).toBe(false);
  });

  it("recognizes config-option apply failures for dismissable notices", () => {
    expect(isSessionConfigChangeFailure("config option fast was refused")).toBe(true);
    expect(isSessionConfigChangeFailure("not advertised on this harness")).toBe(true);
    expect(isSessionConfigChangeFailure("ACP process exited")).toBe(false);
  });
});
