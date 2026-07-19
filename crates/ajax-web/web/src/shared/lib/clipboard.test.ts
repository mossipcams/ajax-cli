import { describe, it, expect, vi, afterEach } from "vitest";
import { copyText } from "./clipboard";

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
