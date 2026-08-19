import { describe, it, expect } from "vitest";
import {
  cleanTitle,
  diffLines,
  formatElapsed,
  middleSplit,
  shortPath,
  toolMark,
  toolStatusNote,
  toolTarget,
} from "./toolPresentation";

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

describe("toolTarget", () => {
  it("reads the path, not the tool's name — every read says 'Read File'", () => {
    expect(
      toolTarget({ title: "Read File", locations: ["/repo/src/session/snapshot.rs"], callId: "c1" }),
    ).toBe("…/session/snapshot.rs");
  });

  it("keeps a command as its own target, without its markdown delimiters", () => {
    expect(toolTarget({ title: "`cargo test`", locations: [], callId: "c1" })).toBe("cargo test");
  });
});

describe("cleanTitle", () => {
  it("strips the backticks a harness sends around a command", () => {
    expect(cleanTitle("`gh issue list`")).toBe("gh issue list");
  });
});

describe("middleSplit", () => {
  it("holds the distinguishing tail aside so only the middle can be lost", () => {
    const [head, tail] = middleSplit("gh issue list --repo mossipcams/ajax-cli --state open");
    expect(tail).toBe("i --state open");
    expect(`${head}${tail}`).toBe("gh issue list --repo mossipcams/ajax-cli --state open");
  });

  it("leaves a short target whole", () => {
    expect(middleSplit("src/lib.rs")).toEqual(["src/lib.rs", ""]);
  });
});

describe("formatElapsed", () => {
  it("says nothing for a replayed burst with no real duration", () => {
    expect(formatElapsed(0)).toBeNull();
    expect(formatElapsed(undefined)).toBeNull();
  });

  it("reads the way an operator would say it", () => {
    expect(formatElapsed(4_200)).toBe("4s");
    expect(formatElapsed(130_000)).toBe("2m 10s");
    expect(formatElapsed(3_720_000)).toBe("1h 2m");
  });
});

describe("toolStatusNote", () => {
  it("spends no word on success and one on everything else", () => {
    expect(toolStatusNote("completed")).toBeNull();
    expect(toolStatusNote("failed")).toBe("failed");
    expect(toolStatusNote("in_progress")).toBe("running");
  });
});
