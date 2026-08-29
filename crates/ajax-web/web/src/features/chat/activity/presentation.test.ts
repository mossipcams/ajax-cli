import { describe, it, expect } from "vitest";
import {
  cleanTitle,
  CONTENT_PREVIEW_LINES,
  diffLines,
  formatElapsed,
  middleSplit,
  OPERATION_VERBS,
  OPERATION_VERBS_PAST,
  shortPath,
  textPreview,
  toolMark,
  toolRowLabel,
  toolRowTarget,
  toolStatusNote,
  toolTarget,
} from "./presentation";

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
    expect(tail).toBe("ajax-cli --state open");
    expect(head).toBe("gh issue list --repo mossipcams/");
    expect(`${head}${tail}`).toBe("gh issue list --repo mossipcams/ajax-cli --state open");
  });

  it("breaks at a token boundary so commands stay readable", () => {
    const command = "cargo nextest run -p ajax-gateway --test gateway::";
    const [head, tail] = middleSplit(command);
    expect(head + tail).toBe(command);
    expect(head.endsWith("…")).toBe(false);
    expect(tail).not.toMatch(/^ures/);
    expect(head).not.toMatch(/-p…$/);
  });

  it("leaves a short target whole", () => {
    expect(middleSplit("src/lib.rs")).toEqual(["src/lib.rs", ""]);
  });
});

describe("toolRowLabel", () => {
  it("reads verb-first with the filename", () => {
    expect(
      toolRowLabel({
        kind: "read",
        title: "Read File",
        locations: ["/repo/crates/gateway/src/serve.rs"],
        callId: "c1",
      }),
    ).toBe("Read serve.rs");
  });

  it("names a search with no location as the verb alone", () => {
    expect(
      toolRowLabel({
        kind: "search",
        title: "Search files",
        locations: [],
        callId: "c1",
      }),
    ).toBe("Searched");
  });

  describe("#1090 tool row targets", () => {
    it("does not duplicate generic Read File into Read Read File", () => {
      expect(
        toolRowLabel({
          kind: "read",
          title: "Read File",
          locations: [],
          callId: "c1",
        }),
      ).toBe("Read");
    });

    it("reads path as Read serve.rs when location is present", () => {
      expect(
        toolRowLabel({
          kind: "read",
          title: "Read File",
          locations: ["/repo/crates/gateway/src/serve.rs"],
          callId: "c1",
        }),
      ).toBe("Read serve.rs");
    });

    it("does not duplicate generic Edit File into Edited Edit File", () => {
      expect(
        toolRowLabel({
          kind: "edit",
          title: "Edit File",
          locations: [],
          callId: "c1",
        }),
      ).toBe("Edited");
    });

    it("shortens execute titles to the first line or clause", () => {
      const dump = `python -c "print('x')" && cargo test\nmore output`;
      expect(
        toolRowLabel({
          kind: "execute",
          title: dump,
          locations: [],
          callId: "c1",
        }),
      ).toBe('Ran python -c "print(\'x\')"');
    });
  });

  it("never gives the same label to read and edit of one path", () => {
    const base = {
      title: "Touch file",
      locations: ["/repo/src/serve.rs"],
      callId: "c1",
    };
    expect(toolRowLabel({ ...base, kind: "read" })).not.toBe(
      toolRowLabel({ ...base, kind: "edit" }),
    );
  });

  it("keeps the live and settled verb maps aligned on kinds", () => {
    expect(Object.keys(OPERATION_VERBS).sort()).toEqual(Object.keys(OPERATION_VERBS_PAST).sort());
  });
});

describe("toolRowTarget", () => {
  it("uses the command when there is no path", () => {
    expect(
      toolRowTarget({
        kind: "execute",
        title: "`cargo nextest run`",
        locations: [],
        callId: "c1",
      }),
    ).toBe("cargo nextest run");
  });
});

describe("textPreview", () => {
  const lines = Array.from({ length: CONTENT_PREVIEW_LINES + 2 }, (_, i) => `row-${i + 1}`).join(
    "\n",
  );

  it("shows the first lines by default", () => {
    const { preview, hiddenLines } = textPreview(lines, CONTENT_PREVIEW_LINES, false);
    expect(preview.startsWith("row-1")).toBe(true);
    expect(preview).not.toContain(`row-${CONTENT_PREVIEW_LINES + 2}`);
    expect(hiddenLines).toBe(2);
  });

  it("shows the last lines for a failure", () => {
    const { preview, hiddenLines } = textPreview(lines, CONTENT_PREVIEW_LINES, true);
    expect(preview.endsWith(`row-${CONTENT_PREVIEW_LINES + 2}`)).toBe(true);
    expect(preview).not.toContain("row-1\n");
    expect(preview.startsWith("row-1")).toBe(false);
    expect(hiddenLines).toBe(2);
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
