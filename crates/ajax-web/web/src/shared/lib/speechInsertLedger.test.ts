import { describe, expect, it } from "vitest";
import {
  clearSpeechInserts,
  prepareSpeechInsert,
  undoPayload,
  type SpeechInsert,
} from "./speechInsertLedger";

describe("speechInsertLedger", () => {
  it("passes through the first insert without a leading space", () => {
    const { textToPaste, record } = prepareSpeechInsert("Hello.", {
      hasPriorInserts: false,
      bracketed: false,
    });

    expect(textToPaste).toBe("Hello.");
    expect(record).toEqual({ text: "Hello.", bracketed: false });
  });

  it("prefixes a space on later inserts unless text already starts with whitespace", () => {
    const second = prepareSpeechInsert("World.", {
      hasPriorInserts: true,
      bracketed: true,
    });
    expect(second.textToPaste).toBe(" World.");
    expect(second.record).toEqual({ text: " World.", bracketed: true });

    const spaced = prepareSpeechInsert("  Next", {
      hasPriorInserts: true,
      textStartsWithWhitespace: true,
      bracketed: false,
    });
    expect(spaced.textToPaste).toBe("  Next");
    expect(spaced.record.text).toBe("  Next");
  });

  it("builds undo payload from plain text lengths and ignores bracketed metadata", () => {
    const records: SpeechInsert[] = [
      { text: "Hello.", bracketed: true },
      { text: " World.", bracketed: false },
    ];

    expect(undoPayload(records)).toBe("\x7f".repeat("Hello. World.".length));
    expect(undoPayload([])).toBe("");
  });

  it("clears the in-memory ledger", () => {
    const records: SpeechInsert[] = [{ text: "Hi", bracketed: false }];
    clearSpeechInserts(records);
    expect(records).toEqual([]);
  });
});
