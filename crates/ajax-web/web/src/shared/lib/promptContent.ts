import type { LivePromptCapabilities } from "./liveSessionPromptCapabilities";

export type PromptContentBlockWire =
  | {
      type: "resource_link";
      name: string;
      uri: string;
      mimeType?: string;
      title?: string;
      description?: string;
    }
  | {
      type: "image";
      data: string;
      mimeType: string;
    }
  | {
      type: "resource";
      uri: string;
      mimeType?: string;
      text?: string;
      blob?: string;
    };

export type ComposerAttachment = {
  id: string;
  label: string;
  blocks: PromptContentBlockWire[];
};

/** Match host `ws_bridge::MAX_SESSION_FRAME_BYTES`. */
export const MAX_PROMPT_FRAME_BYTES = 256 * 1024;

export const ATTACHMENT_TOO_LARGE =
  "That attachment is too large to send even after compression. Remove it or choose a smaller file.";

const PROMPT_FRAME_HEADROOM_BYTES = 4096;
const PLACEHOLDER_CLIENT_MESSAGE_ID = "00000000-0000-4000-8000-000000000000";

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== "string") {
        reject(new Error("Could not read file"));
        return;
      }
      resolve(reader.result);
    };
    reader.onerror = () => reject(reader.error ?? new Error("Could not read file"));
    reader.readAsDataURL(file);
  });
}

function readFileAsText(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === "string" ? reader.result : "");
    reader.onerror = () => reject(reader.error ?? new Error("Could not read file"));
    reader.readAsText(file);
  });
}

function readFileAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (!(reader.result instanceof ArrayBuffer)) {
        reject(new Error("Could not read file"));
        return;
      }
      const bytes = new Uint8Array(reader.result);
      let binary = "";
      for (const byte of bytes) binary += String.fromCharCode(byte);
      resolve(btoa(binary));
    };
    reader.onerror = () => reject(reader.error ?? new Error("Could not read file"));
    reader.readAsArrayBuffer(file);
  });
}

function dataUrlPayload(dataUrl: string): { mimeType: string; data: string } | null {
  const match = /^data:([^;,]+);base64,(.+)$/i.exec(dataUrl);
  if (!match) return null;
  return { mimeType: match[1], data: match[2] };
}

function attachmentId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function promptFrameJson(text: string, contentBlocks: PromptContentBlockWire[]): string {
  return JSON.stringify({
    type: "prompt",
    text,
    clientMessageId: PLACEHOLDER_CLIENT_MESSAGE_ID,
    ...(contentBlocks.length ? { contentBlocks } : {}),
  });
}

export function estimatePromptFrameBytes(
  text: string,
  contentBlocks: PromptContentBlockWire[],
): number {
  return new TextEncoder().encode(promptFrameJson(text.trim(), contentBlocks)).length;
}

export function promptFrameFits(text: string, contentBlocks: PromptContentBlockWire[]): boolean {
  return estimatePromptFrameBytes(text, contentBlocks) <= MAX_PROMPT_FRAME_BYTES;
}

export function canAttachFiles(caps: LivePromptCapabilities | undefined): boolean {
  return Boolean(caps?.image || caps?.embeddedContext);
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("Could not decode image"));
    img.src = src;
  });
}

async function compressImageElement(
  img: HTMLImageElement,
  maxBase64Chars: number,
): Promise<{ data: string; mimeType: string } | null> {
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;

  let width = img.naturalWidth || img.width;
  let height = img.naturalHeight || img.height;
  if (!width || !height) return null;

  let quality = 0.92;
  for (let attempt = 0; attempt < 16; attempt += 1) {
    canvas.width = width;
    canvas.height = height;
    ctx.drawImage(img, 0, 0, width, height);
    const payload = dataUrlPayload(canvas.toDataURL("image/jpeg", quality));
    if (payload && payload.data.length <= maxBase64Chars) {
      return { data: payload.data, mimeType: "image/jpeg" };
    }
    if (quality > 0.45) {
      quality -= 0.08;
      continue;
    }
    width = Math.max(1, Math.floor(width * 0.75));
    height = Math.max(1, Math.floor(height * 0.75));
    quality = 0.88;
  }
  return null;
}

async function recompressImageBlock(
  block: Extract<PromptContentBlockWire, { type: "image" }>,
  maxBase64Chars: number,
): Promise<Extract<PromptContentBlockWire, { type: "image" }> | null> {
  if (block.data.length <= maxBase64Chars) return block;
  const binary = atob(block.data);
  const bytes = Uint8Array.from(binary, (byte) => byte.charCodeAt(0));
  const blob = new Blob([bytes], { type: block.mimeType || "image/jpeg" });
  const url = URL.createObjectURL(blob);
  try {
    const img = await loadImage(url);
    const compressed = await compressImageElement(img, maxBase64Chars);
    if (!compressed) return null;
    return { type: "image", data: compressed.data, mimeType: compressed.mimeType };
  } finally {
    URL.revokeObjectURL(url);
  }
}

function maxImageDataLength(text: string, blocks: PromptContentBlockWire[]): number {
  const nonImageBlocks = blocks.filter((block) => block.type !== "image");
  const emptyImageBlock: PromptContentBlockWire = { type: "image", data: "", mimeType: "image/jpeg" };
  const frameWithoutImageData = estimatePromptFrameBytes(text, [...nonImageBlocks, emptyImageBlock]);
  const budget =
    MAX_PROMPT_FRAME_BYTES - PROMPT_FRAME_HEADROOM_BYTES - Math.max(0, frameWithoutImageData);
  return Math.max(0, budget);
}

export async function fitPromptContentBlocks(
  text: string,
  blocks: PromptContentBlockWire[],
): Promise<{ blocks: PromptContentBlockWire[]; error?: string }> {
  if (promptFrameFits(text, blocks)) return { blocks };

  const maxDataLen = maxImageDataLength(text, blocks);
  const next: PromptContentBlockWire[] = [];
  for (const block of blocks) {
    if (block.type !== "image") {
      next.push(block);
      continue;
    }
    const compressed = await recompressImageBlock(block, maxDataLen);
    if (!compressed) return { blocks, error: ATTACHMENT_TOO_LARGE };
    next.push(compressed);
  }

  if (promptFrameFits(text, next)) return { blocks: next };
  return { blocks, error: ATTACHMENT_TOO_LARGE };
}

export async function attachmentFromFile(
  file: File,
  caps: LivePromptCapabilities | undefined,
): Promise<ComposerAttachment | null> {
  if (!canAttachFiles(caps)) return null;

  const mimeType = file.type || "application/octet-stream";
  const name = file.name.trim() || "attachment";
  const uri = `file:///${encodeURIComponent(name)}`;

  if (mimeType.startsWith("image/") && caps?.image) {
    const dataUrl = await readFileAsDataUrl(file);
    const payload = dataUrlPayload(dataUrl);
    if (!payload) return null;
    return {
      id: attachmentId("image"),
      label: name,
      blocks: [{ type: "image", data: payload.data, mimeType: payload.mimeType }],
    };
  }

  if (caps?.embeddedContext && mimeType.startsWith("text/")) {
    const text = await readFileAsText(file);
    return {
      id: attachmentId("resource"),
      label: name,
      blocks: [{ type: "resource", uri, mimeType, text }],
    };
  }

  if (caps?.embeddedContext) {
    const blob = await readFileAsBase64(file);
    return {
      id: attachmentId("resource"),
      label: name,
      blocks: [{ type: "resource", uri, mimeType, blob }],
    };
  }

  return null;
}

export async function attachmentFromPaste(
  item: DataTransferItem,
  caps: LivePromptCapabilities | undefined,
): Promise<ComposerAttachment | null> {
  if (item.kind !== "file") return null;
  const file = item.getAsFile();
  if (!file) return null;
  return attachmentFromFile(file, caps);
}

export function flattenAttachmentBlocks(attachments: ComposerAttachment[]): PromptContentBlockWire[] {
  return attachments.flatMap((attachment) => attachment.blocks);
}

export function attachmentsFromContentBlocks(blocks: PromptContentBlockWire[]): ComposerAttachment[] {
  return blocks.map((block, index) => ({
    id: attachmentId(`restored-${index}`),
    label:
      block.type === "resource_link"
        ? block.name
        : block.type === "image"
          ? "Image"
          : block.uri.split("/").pop() || "Attachment",
    blocks: [block],
  }));
}
