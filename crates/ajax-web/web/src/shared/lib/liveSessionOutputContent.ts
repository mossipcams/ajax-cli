export type OutputContentBlock =
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
      mimeType: string;
      uri?: string;
      data?: string;
    }
  | {
      type: "resource";
      uri: string;
      mimeType?: string;
      text?: string;
      blob?: string;
    };

export type ToolContent =
  | { type: "text"; text: string }
  | { type: "diff"; path: string; oldText?: string | null; newText: string }
  | { type: "image"; mimeType: string; uri?: string; data?: string }
  | {
      type: "resource_link";
      name: string;
      uri: string;
      mimeType?: string;
      title?: string;
      description?: string;
    }
  | {
      type: "resource";
      uri: string;
      mimeType?: string;
      text?: string;
      blob?: string;
    };

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object";
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

export function parseOutputContentBlock(value: unknown): OutputContentBlock | null {
  if (!isRecord(value) || typeof value.type !== "string") return null;
  switch (value.type) {
    case "resource_link":
      if (typeof value.name !== "string" || typeof value.uri !== "string") return null;
      return {
        type: "resource_link",
        name: value.name,
        uri: value.uri,
        ...(optionalString(value.mimeType) ? { mimeType: value.mimeType as string } : {}),
        ...(optionalString(value.title) ? { title: value.title as string } : {}),
        ...(optionalString(value.description) ? { description: value.description as string } : {}),
      };
    case "image":
      if (typeof value.mimeType !== "string") return null;
      return {
        type: "image",
        mimeType: value.mimeType,
        ...(optionalString(value.uri) ? { uri: value.uri as string } : {}),
        ...(optionalString(value.data) ? { data: value.data as string } : {}),
      };
    case "resource":
      if (typeof value.uri !== "string") return null;
      return {
        type: "resource",
        uri: value.uri,
        ...(optionalString(value.mimeType) ? { mimeType: value.mimeType as string } : {}),
        ...(optionalString(value.text) ? { text: value.text as string } : {}),
        ...(optionalString(value.blob) ? { blob: value.blob as string } : {}),
      };
    default:
      return null;
  }
}

export function parseOutputContentBlocks(value: unknown): OutputContentBlock[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => parseOutputContentBlock(item))
    .filter((item): item is OutputContentBlock => item !== null);
}

export function parseToolContent(value: unknown): ToolContent | null {
  if (!isRecord(value) || typeof value.type !== "string") return null;
  switch (value.type) {
    case "text":
      return typeof value.text === "string" ? { type: "text", text: value.text } : null;
    case "diff":
      if (typeof value.path !== "string" || typeof value.newText !== "string") return null;
      return {
        type: "diff",
        path: value.path,
        newText: value.newText,
        ...(value.oldText === null || typeof value.oldText === "string"
          ? { oldText: value.oldText as string | null }
          : {}),
      };
    case "terminal":
      return null;
    default: {
      const block = parseOutputContentBlock(value);
      return block ? (block as ToolContent) : null;
    }
  }
}

export function parseToolContentList(value: unknown): ToolContent[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => parseToolContent(item))
    .filter((item): item is ToolContent => item !== null);
}

export function imageSource(block: Extract<OutputContentBlock, { type: "image" }>): string | null {
  if (block.uri) return block.uri;
  if (block.data) return `data:${block.mimeType};base64,${block.data}`;
  return null;
}

export function resourceLabel(block: Extract<OutputContentBlock, { type: "resource_link" }>): string {
  return block.title?.trim() || block.name;
}

export function embeddedResourceLabel(
  block: Extract<OutputContentBlock, { type: "resource" }>,
): string {
  const tail = block.uri.split("/").filter(Boolean).pop();
  return tail || block.uri;
}
