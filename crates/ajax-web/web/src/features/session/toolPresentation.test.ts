import { describe, it, expect } from "vitest";
import { diffLines, shortPath, toolMark } from "./toolPresentation";

describe("diffLines", () => {
  it("shows only the changed span, with context, out of a whole file", () => {
    const before = ["a", "b", "c", "d", "OLD", "e", "f", "g"].join("\n");
    const after = ["a", "b", "c", "d", "NEW", "e", "f", "g"].join("\n");

    expect(diffLines(before, after)).toEqual([
      { sign: " ", text: "c" },
      { sign: " ", text: "d" },
      { sign: "-", text: "OLD" },
      { sign: "+", text: "NEW" },
      { sign: " ", text: "e" },
      { sign: " ", text: "f" },
    ]);
  });

  it("renders a new file as pure addition", () => {
    expect(diffLines("", "fn main() {}")).toEqual([{ sign: "+", text: "fn main() {}" }]);
  });

  it("renders a deletion with no replacement", () => {
    expect(diffLines("gone", "")).toEqual([{ sign: "-", text: "gone" }]);
  });

  it("reports no change when the two sides match", () => {
    expect(diffLines("same\n", "same\n")).toEqual([]);
  });
});

describe("shortPath", () => {
  it("keeps the informative tail of a long path", () => {
    expect(shortPath("/repo/crates/ajax-web/src/lib.rs")).toBe("…/src/lib.rs");
  });

  it("leaves a short path alone", () => {
    expect(shortPath("src/lib.rs")).toBe("src/lib.rs");
  });
});

describe("toolMark", () => {
  it("falls back for a kind the protocol added after us", () => {
    expect(toolMark("edit")).toBe("±");
    expect(toolMark("something_new")).toBe("•");
  });
});
