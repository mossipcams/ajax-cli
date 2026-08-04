import { describe, expect, it } from "vitest";
import { sanitizeTelemetryProps } from "./telemetryFilter";

describe("sanitizeTelemetryProps", () => {
  it("keeps safe telemetry properties", () => {
    expect(
      sanitizeTelemetryProps({
        control: "drop",
        ok: true,
        duration_ms: 120,
        direction: "left",
      }),
    ).toEqual({
      control: "drop",
      ok: true,
      duration_ms: 120,
      direction: "left",
    });
  });

  it("drops sensitive keys", () => {
    expect(
      sanitizeTelemetryProps({
        control: "run",
        terminal_output: "secret buffer",
        prompt_text: ">",
        api_token: "abc",
      }),
    ).toEqual({ control: "run" });
  });

  it("drops suspicious string values", () => {
    expect(
      sanitizeTelemetryProps({
        control: "run",
        note: "git commit -m fix",
      }),
    ).toEqual({ control: "run" });

    expect(
      sanitizeTelemetryProps({
        control: "run",
        note: "phc_uQFMpY3C9L9Dj4wLqudjNyJVBwAdCisMyUkZ6EqhxWxB",
      }),
    ).toEqual({ control: "run" });

    expect(
      sanitizeTelemetryProps({
        control: "run",
        note: "line1\nline2 with terminal-like multiline output content",
      }),
    ).toEqual({ control: "run" });
  });

  it("drops null and undefined values", () => {
    expect(
      sanitizeTelemetryProps({
        control: "drop",
        error_kind: undefined,
        op: null,
      }),
    ).toEqual({ control: "drop" });
  });
});
