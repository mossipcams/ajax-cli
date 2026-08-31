import { describe, expect, it, vi } from "vitest";
import {
  ATTACHMENT_TOO_LARGE,
  attachmentFromFile,
  estimatePromptFrameBytes,
  fitPromptContentBlocks,
  flattenAttachmentBlocks,
  hasPromptContent,
  MAX_IMAGE_BLOCKS,
  MAX_PROMPT_FRAME_BYTES,
  promptFrameFits,
} from "./promptContent";
import { MAX_FRAME_BYTES } from "@/features/chat/session/transport/contracts";

describe("promptContent", () => {
  it("shares the bounded 8 MiB frame limit with transport contracts", () => {
    expect(MAX_PROMPT_FRAME_BYTES).toBe(MAX_FRAME_BYTES);
    expect(MAX_PROMPT_FRAME_BYTES).toBe(8 * 1024 * 1024);
    expect(MAX_IMAGE_BLOCKS).toBe(8);
  });

  it("treats image-only drafts as having content", () => {
    const blocks = [{ type: "image" as const, data: "aGVsbG8=", mimeType: "image/png" }];
    expect(hasPromptContent("", blocks)).toBe(true);
    expect(hasPromptContent("   ", [])).toBe(false);
    expect(hasPromptContent("", [])).toBe(false);
  });

  it("passes through photos larger than the legacy 256 KiB cap when the frame fits", () => {
    const legacyLarge = "A".repeat(300_000);
    const blocks = [{ type: "image" as const, data: legacyLarge, mimeType: "image/jpeg" }];
    expect(legacyLarge.length).toBeGreaterThan(256 * 1024);
    expect(promptFrameFits("hello", blocks)).toBe(true);
  });

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
    const hugeData = "A".repeat(MAX_PROMPT_FRAME_BYTES);
    const blocks = [{ type: "image" as const, data: hugeData, mimeType: "image/jpeg" }];
    expect(estimatePromptFrameBytes("hello", blocks)).toBeGreaterThan(MAX_PROMPT_FRAME_BYTES);

    const createElement = Document.prototype.createElement;
    vi.spyOn(document, "createElement").mockImplementation(function (this: Document, tagName: string) {
      const element = createElement.call(this, tagName);
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

  it("reports attachment too large when compression cannot fit the frame", async () => {
    const hugeData = "A".repeat(MAX_PROMPT_FRAME_BYTES);
    const blocks = [{ type: "image" as const, data: hugeData, mimeType: "image/jpeg" }];

    const createElement = Document.prototype.createElement;
    vi.spyOn(document, "createElement").mockImplementation(function (this: Document, tagName: string) {
      const element = createElement.call(this, tagName);
      if (tagName !== "canvas") return element;
      const canvas = element as HTMLCanvasElement;
      canvas.getContext = vi.fn(() => ({
        drawImage: vi.fn(),
      })) as unknown as HTMLCanvasElement["getContext"];
      canvas.toDataURL = vi.fn(() => `data:image/jpeg;base64,${"B".repeat(MAX_PROMPT_FRAME_BYTES)}`);
      return canvas;
    });
    vi.spyOn(globalThis, "Image").mockImplementation(function MockImage(this: HTMLImageElement) {
      Object.defineProperty(this, "naturalWidth", { value: 4000 });
      Object.defineProperty(this, "naturalHeight", { value: 3000 });
      setTimeout(() => this.onload?.(new Event("load")), 0);
      return this;
    } as unknown as typeof Image);

    const fitted = await fitPromptContentBlocks("", blocks);
    expect(fitted.error).toBe(ATTACHMENT_TOO_LARGE);
    expect(promptFrameFits("", fitted.blocks)).toBe(false);
  });
});
