import { describe, expect, it, vi } from "vitest";
import { parseTranscriptPage } from "./parse";
import { fetchEarlierTranscript, type TranscriptPage } from "./public";

function frame(cursor: number) {
  return {
    type: "event",
    protocolVersion: 2,
    cursor,
    payload: { type: "message", role: "assistant", text: `Event ${cursor}` },
  };
}

function page() {
  return { protocolVersion: 2, events: [frame(10), frame(12)], from: 10, floor: 3 };
}

function fetchReturning(body: unknown) {
  return vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(body)));
}

describe("fetchEarlierTranscript", () => {
  it("encodes a slash in the handle and omits an unspecified limit", async () => {
    const fetchImpl = fetchReturning(page());
    await fetchEarlierTranscript("repo/task", 20, { fetchImpl });
    expect(fetchImpl).toHaveBeenCalledExactlyOnceWith(
      "/api/tasks/repo%2Ftask/session/transcript?before=20",
      { method: "GET", signal: undefined },
    );
  });

  it("includes a supplied limit", async () => {
    const fetchImpl = fetchReturning(page());
    await fetchEarlierTranscript("task", 20, { limit: 5, fetchImpl });
    expect(fetchImpl).toHaveBeenCalledExactlyOnceWith(
      "/api/tasks/task/session/transcript?before=20&limit=5",
      { method: "GET", signal: undefined },
    );
  });

  it("parses a valid 200 page into transcript events", async () => {
    const result: TranscriptPage = await fetchEarlierTranscript("task", 20, {
      fetchImpl: fetchReturning(page()),
    });
    expect(result).toEqual({
      events: [
        { cursor: 10, event: { type: "message", role: "assistant", text: "Event 10" } },
        { cursor: 12, event: { type: "message", role: "assistant", text: "Event 12" } },
      ],
      from: 10,
      floor: 3,
    });
  });

  it("accepts an empty page at the retention floor", async () => {
    expect(await fetchEarlierTranscript("task", 20, {
      fetchImpl: fetchReturning({ protocolVersion: 2, events: [], from: 3, floor: 3 }),
    })).toEqual({ events: [], from: 3, floor: 3 });
  });

  it("accepts a lower bound before the first event", async () => {
    expect(await fetchEarlierTranscript("task", 20, {
      fetchImpl: fetchReturning({ ...page(), from: 3 }),
    })).toMatchObject({ from: 3, floor: 3 });
  });

  it("throws an error containing the 404 status", async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(null, { status: 404 }));
    await expect(fetchEarlierTranscript("task", 20, { fetchImpl })).rejects.toThrow("404");
  });

  it.each([
    ["wrong protocolVersion", { ...page(), protocolVersion: 1 }],
    ["descending cursors", { ...page(), events: [frame(12), frame(10)] }],
    ["duplicate cursors", { ...page(), events: [frame(10), frame(10)] }],
    ["floor greater than from", { ...page(), floor: 11 }],
    ["from greater than first cursor", { ...page(), from: 11 }],
    ["missing events", { protocolVersion: 2, from: 10, floor: 3 }],
    ["non-array events", { ...page(), events: {} }],
    ["null body", null],
    ["array body", []],
    ["null event", { ...page(), events: [null] }],
    ["wrong event protocolVersion", { ...page(), events: [{ ...frame(10), protocolVersion: 1 }] }],
    ["wrong frame type", { ...page(), events: [{ ...frame(10), type: "snapshot" }] }],
    ["invalid event payload", { ...page(), events: [{ ...frame(10), payload: { type: "unknown" } }] }],
    ["negative cursor", { ...page(), events: [frame(-1)] }],
    ["fractional cursor", { ...page(), events: [frame(10.5)] }],
    ["unsafe cursor", { ...page(), events: [frame(Number.MAX_SAFE_INTEGER + 1)] }],
    ["negative from", { ...page(), from: -1 }],
    ["fractional from", { ...page(), from: 3.5 }],
    ["unsafe from", { ...page(), from: Number.MAX_SAFE_INTEGER + 1 }],
    ["string from", { ...page(), from: "10" }],
    ["negative floor", { ...page(), floor: -1 }],
    ["fractional floor", { ...page(), floor: 1.5 }],
    ["unsafe floor", { ...page(), floor: Number.MAX_SAFE_INTEGER + 1 }],
    ["string floor", { ...page(), floor: "3" }],
  ])("rejects %s as an invalid transcript page", async (_name, body) => {
    expect(parseTranscriptPage(body)).toBeNull();
    await expect(fetchEarlierTranscript("task", 20, {
      fetchImpl: fetchReturning(body),
    })).rejects.toThrow("invalid transcript page");
  });

  it.each([20, 21])("rejects cursor %s at or above before", async (cursor) => {
    await expect(fetchEarlierTranscript("task", 20, {
      fetchImpl: fetchReturning({ ...page(), events: [frame(10), frame(cursor)] }),
    })).rejects.toThrow("invalid transcript page");
  });

  it("rejects malformed JSON as an invalid transcript page", async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response("{"));
    await expect(fetchEarlierTranscript("task", 20, { fetchImpl }))
      .rejects.toThrow("invalid transcript page");
  });

  it("passes through the abort signal and propagates a fetch abort", async () => {
    const controller = new AbortController();
    controller.abort();
    const error = new DOMException("Aborted", "AbortError");
    const fetchImpl = vi.fn<typeof fetch>().mockRejectedValue(error);
    await expect(fetchEarlierTranscript("task", 20, {
      fetchImpl, signal: controller.signal,
    })).rejects.toBe(error);
    expect(fetchImpl).toHaveBeenCalledExactlyOnceWith(
      "/api/tasks/task/session/transcript?before=20",
      { method: "GET", signal: controller.signal },
    );
  });

  it("propagates an abort while reading the response body", async () => {
    const error = new DOMException("Aborted", "AbortError");
    const response = new Response();
    vi.spyOn(response, "json").mockRejectedValue(error);
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(response);
    await expect(fetchEarlierTranscript("task", 20, { fetchImpl })).rejects.toBe(error);
  });
});
