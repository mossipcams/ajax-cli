import { describe, it, expect } from "vitest";
import { isSessionModelChangeFailure } from "./sessionModel";

describe("sessionModel", () => {
  it("recognizes host errors that should revert an in-session model change (#942)", () => {
    expect(isSessionModelChangeFailure("session model change needs a task Ajax started over ACP")).toBe(
      true,
    );
    expect(isSessionModelChangeFailure("session model composer-2.5 was refused — model refused")).toBe(
      true,
    );
    expect(isSessionModelChangeFailure("session model composer-2.5 could not be verified — harness did not report an applied model")).toBe(
      true,
    );
    expect(isSessionModelChangeFailure("unsupported model")).toBe(true);
    expect(isSessionModelChangeFailure("ACP process exited")).toBe(false);
    expect(isSessionModelChangeFailure("queued prompt failed: prompt already in flight")).toBe(false);
  });
});
