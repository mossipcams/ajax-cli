import { describe, expect, it, vi } from "vitest";
import {
  attachmentFromFile,
  estimatePromptFrameBytes,
  fitPromptContentBlocks,
  flattenAttachmentBlocks,
  MAX_PROMPT_FRAME_BYTES,
  promptFrameFits,
} from "./promptContent";

describe("promptContent", () => {
  it("returns null when neither image nor embeddedContext is advertised", async () => {
    const file = new File(["hello"], "notes.md", { type: "text/markdown" });
    await expect(
      attachmentFromFile(file, { image: false, embeddedContext: false }),
    ).resolves.toBeNull();
  });

  it("embeds text files when embeddedContext is advertised", async () => {
    const file = new File(["hello"], "notes.txt", { type: "text/plain" });
    const attachment = await attachmentFromFile(file, { embeddedContext: true });
    expect(attachment?.blocks[0]).toMatchObject({
      type: "resource",
      text: "hello",
    });
  });

  it("flattens attachment blocks in order", () => {
    const blocks = flattenAttachmentBlocks([
      {
        id: "a",
        label: "one",
        blocks: [{ type: "resource_link", name: "one", uri: "file:///one" }],
      },
      {
        id: "b",
        label: "two",
        blocks: [{ type: "image", data: "aGVsbG8=", mimeType: "image/png" }],
      },
    ]);
    expect(blocks).toHaveLength(2);
  });

  it("compresses a large image attachment so the prompt frame fits", async () => {
    const hugeData = "A".repeat(300_000);
    const blocks = [{ type: "image" as const, data: hugeData, mimeType: "image/jpeg" }];
    expect(estimatePromptFrameBytes("hello", blocks)).toBeGreaterThan(MAX_PROMPT_FRAME_BYTES);

    const originalCreateElement = document.createElement.bind(document);
    vi.spyOn(document, "createElement").mockImplementation((tagName: string) => {
      const element = originalCreateElement(tagName);
      if (tagName !== "canvas") return element;
      const canvas = element as HTMLCanvasElement;
      canvas.getContext = vi.fn(() => ({
        drawImage: vi.fn(),
      })) as unknown as HTMLCanvasElement["getContext"];
      canvas.toDataURL = vi.fn((_type?: string, quality?: number) => {
        const scale = typeof quality === "number" ? quality : 0.92;
        const targetLen = Math.max(512, Math.floor(1200 * scale));
        return `data:image/jpeg;base64,${"B".repeat(targetLen)}`;
      });
      return canvas;
    });
    vi.spyOn(globalThis, "Image").mockImplementation(function MockImage(this: HTMLImageElement) {
      Object.defineProperty(this, "naturalWidth", { value: 4000 });
      Object.defineProperty(this, "naturalHeight", { value: 3000 });
      setTimeout(() => this.onload?.(new Event("load")), 0);
      return this;
    } as unknown as typeof Image);

    const fitted = await fitPromptContentBlocks("hello", blocks);
    expect(fitted.error).toBeUndefined();
    expect(promptFrameFits("hello", fitted.blocks)).toBe(true);
  });
});
