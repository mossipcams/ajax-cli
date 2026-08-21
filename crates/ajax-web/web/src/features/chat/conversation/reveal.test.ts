import { describe, it, expect } from "vitest";
import { settledText } from "./reveal";

describe("settledText", () => {
  it("returns complete paragraphs only", () => {
    expect(settledText("First paragraph.\n\nSecond partial")).toBe("First paragraph.");
  });

  it("returns empty when no paragraph break exists yet", () => {
    expect(settledText("Still streaming")).toBe("");
  });

  it("never cuts inside a fenced block", () => {
    expect(settledText("Here:\n\n```sh\ncargo test\n\nnpm run web:test")).toBe("Here:");
  });
});
