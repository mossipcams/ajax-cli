import type { SessionTranscriptEvent } from "./contracts";
import { parseTranscriptPage } from "./parse";

export type TranscriptPage = {
  events: SessionTranscriptEvent[];
  from: number;
  floor: number;
};

export async function fetchEarlierTranscript(
  handle: string,
  before: number,
  options: { limit?: number; fetchImpl?: typeof fetch; signal?: AbortSignal } = {},
): Promise<TranscriptPage> {
  const url = `/api/tasks/${encodeURIComponent(handle)}/session/transcript?before=${before}`
    + (options.limit !== undefined ? `&limit=${options.limit}` : "");
  const response = await (options.fetchImpl ?? fetch)(url, {
    method: "GET",
    signal: options.signal,
  });
  if (!response.ok) {
    throw new Error(`transcript request failed: ${response.status}`);
  }
  let payload: unknown;
  try {
    payload = await response.json();
  } catch (error) {
    if (error instanceof SyntaxError) throw new Error("invalid transcript page");
    throw error;
  }
  const page = parseTranscriptPage(payload);
  if (!page || page.events.some(({ cursor }) => cursor >= before)) {
    throw new Error("invalid transcript page");
  }
  return page;
}
