import { describe, expect, it } from "vitest";
import { parseLiveSessionTitle } from "./liveSessionTitle";

describe("parseLiveSessionTitle", () => {
  it("accepts trimmed non-empty strings", () => {
    expect(parseLiveSessionTitle(" Fix auth flow ")).toBe("Fix auth flow");
  });

  it("rejects empty and non-string values", () => {
    expect(parseLiveSessionTitle("")).toBeUndefined();
    expect(parseLiveSessionTitle("   ")).toBeUndefined();
    expect(parseLiveSessionTitle(null)).toBeUndefined();
  });
});
