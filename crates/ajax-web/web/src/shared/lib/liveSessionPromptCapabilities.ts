/** Advertised ACP prompt content capabilities from protocol v2 `promptCapabilities`. */
export interface LivePromptCapabilities {
  image?: boolean;
  embeddedContext?: boolean;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object";
}

export function parseLivePromptCapabilities(raw: unknown): LivePromptCapabilities | undefined {
  if (!isRecord(raw)) return undefined;
  const image = raw.image === true;
  const embeddedContext = raw.embeddedContext === true;
  if (!image && !embeddedContext) {
    return { image: false, embeddedContext: false };
  }
  return {
    ...(image ? { image: true } : {}),
    ...(embeddedContext ? { embeddedContext: true } : {}),
  };
}

export function allowsImageAttachments(caps: LivePromptCapabilities | undefined): boolean {
  return caps?.image === true;
}

export function allowsEmbeddedResources(caps: LivePromptCapabilities | undefined): boolean {
  return caps?.embeddedContext === true;
}
