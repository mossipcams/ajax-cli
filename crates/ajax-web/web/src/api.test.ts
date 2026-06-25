import { describe, it, expect, vi, afterEach } from "vitest";
import { ApiError, postAnswer, postOperation, fetchCockpit } from "./api";

function mockFetch(impl: () => Promise<Response> | Response) {
  vi.stubGlobal("fetch", vi.fn(impl));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const validCockpit = {
  backend: { authority: "host-native", control_enabled: true },
  repos: { repos: [] },
  cards: [],
  inbox: { items: [] },
};

describe("fetchCockpit", () => {
  it("returns a validated cockpit on success", async () => {
    mockFetch(() => json(validCockpit));
    const cockpit = await fetchCockpit();
    expect(cockpit.cards).toEqual([]);
  });

  it("raises an incompatible-response error on malformed JSON shape", async () => {
    mockFetch(() => json({ nope: true }));
    await expect(fetchCockpit()).rejects.toMatchObject({ kind: "incompatible" });
  });

  it("raises a network error when fetch rejects", async () => {
    mockFetch(() => Promise.reject(new Error("offline")));
    await expect(fetchCockpit()).rejects.toMatchObject({ kind: "network" });
  });
});

describe("postAnswer status mapping", () => {
  it("maps 409 to a conflict error", async () => {
    mockFetch(() => json({}, 409));
    await expect(
      postAnswer("web/x", { answer: "approve", fingerprint: "f", request_id: "r" }),
    ).rejects.toMatchObject({ kind: "conflict" });
  });

  it("maps 422 to a terminal-escalation error", async () => {
    mockFetch(() => json({}, 422));
    await expect(
      postAnswer("web/x", { answer: "deny", fingerprint: "f", request_id: "r" }),
    ).rejects.toMatchObject({ kind: "terminal" });
  });

  it("maps 429 to a rate-limit error", async () => {
    mockFetch(() => json({}, 429));
    await expect(
      postAnswer("web/x", { answer: "approve", fingerprint: "f", request_id: "r" }),
    ).rejects.toMatchObject({ kind: "rate-limit" });
  });
});

describe("postOperation", () => {
  it("returns the refreshed cockpit projection on success", async () => {
    mockFetch(() => json({ cockpit: validCockpit, output: "done" }));
    const result = await postOperation({
      task_handle: "web/x",
      action: "review",
      request_id: "r",
    });
    expect(result.ok).toBe(true);
    expect(result.response.cockpit?.cards).toEqual([]);
  });

  it("surfaces a non-JSON error body as an http error", async () => {
    mockFetch(
      () => new Response("boom", { status: 500, headers: { "content-type": "text/plain" } }),
    );
    const result = await postOperation({
      task_handle: "web/x",
      action: "review",
      request_id: "r",
    });
    expect(result.ok).toBe(false);
    expect(result.error).toBeInstanceOf(ApiError);
  });
});
