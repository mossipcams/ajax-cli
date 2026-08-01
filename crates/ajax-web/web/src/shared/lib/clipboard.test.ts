import { describe, it, expect, vi, afterEach } from "vitest";
import { copyText, readPasteText } from "./clipboard";

function pasteData(parts: Record<string, string>): DataTransfer {
  const store = new Map<string, string>(Object.entries(parts));
  return {
    getData: (type: string) => store.get(type) ?? "",
    setData: (type: string, value: string) => {
      store.set(type, value);
    },
  } as DataTransfer;
}

describe("readPasteText", () => {
  it("returns a plain http(s) URL unchanged", () => {
    expect(readPasteText(pasteData({ "text/plain": "https://example.com/c" }))).toBe(
      "https://example.com/c",
    );
  });

  it("reads uri-list-only paste as the URL", () => {
    expect(
      readPasteText(pasteData({ "text/uri-list": "https://example.com/a", "text/plain": "" })),
    ).toBe("https://example.com/a");
  });

  it("reads html-href-only paste as the href URL", () => {
    expect(
      readPasteText(
        pasteData({
          "text/html": '<a href="https://example.com/b">label</a>',
          "text/plain": "",
        }),
      ),
    ).toBe("https://example.com/b");
  });

  it("prefers an http(s) href when plain text is only a link title", () => {
    expect(
      readPasteText(
        pasteData({
          "text/plain": "Click here",
          "text/html": '<a href="https://example.com/d">Click here</a>',
        }),
      ),
    ).toBe("https://example.com/d");
  });

  it("keeps plain text that starts with a URL plus trailing words", () => {
    expect(
      readPasteText(pasteData({ "text/plain": "https://example.com/c see also" })),
    ).toBe("https://example.com/c see also");
  });

  it("does not paste raw HTML when no href is present", () => {
    expect(
      readPasteText(pasteData({ "text/html": "<b>not a link</b>", "text/plain": "" })),
    ).toBe("");
  });

  it("returns empty string for empty clipboard data", () => {
    expect(readPasteText(pasteData({}))).toBe("");
    expect(readPasteText(null)).toBe("");
  });
});

describe("copyText", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns true when clipboard accepts the write", async () => {
    vi.stubGlobal("navigator", {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });

    const ok = await copyText("hello");
    expect(ok).toBe(true);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("hello");
  });

  it("returns true via execCommand when clipboard API is unavailable", async () => {
    vi.stubGlobal("navigator", {});
    const execCommand = vi.fn().mockReturnValue(true);
    Object.defineProperty(document, "execCommand", {
      value: execCommand,
      configurable: true,
    });
    const textareasBefore = document.body.querySelectorAll("textarea").length;

    const ok = await copyText("plain-http copy");

    expect(ok).toBe(true);
    expect(execCommand).toHaveBeenCalledWith("copy");
    expect(document.body.querySelectorAll("textarea").length).toBe(textareasBefore);
    Reflect.deleteProperty(document, "execCommand");
  });

  it("returns false when clipboard API is unavailable", async () => {
    vi.stubGlobal("navigator", {});

    const ok = await copyText("hello");
    expect(ok).toBe(false);
  });
});
