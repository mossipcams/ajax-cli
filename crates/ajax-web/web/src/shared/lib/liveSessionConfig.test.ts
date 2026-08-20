import { describe, expect, it } from "vitest";
import {
  encodeDesiredPinFromLiveOptions,
  encodeDesiredPinWithLiveSelection,
  modelLiveOption,
  parseLiveConfigOptions,
} from "./liveSessionConfig";

const PARAMETERIZED = [
  {
    id: "model",
    category: "model",
    name: "Model",
    type: "select",
    currentValue: "grok-4.6",
    choices: [
      { value: "grok-4.6", name: "Grok 4.6" },
      { value: "composer-2.5", name: "Composer 2.5" },
    ],
  },
  {
    id: "reasoning",
    category: "thought_level",
    name: "Effort",
    type: "select",
    currentValue: "high",
    choices: [
      { value: "high", name: "High" },
      { value: "low", name: "Low" },
    ],
  },
  {
    id: "fast",
    category: "model_config",
    name: "Fast",
    type: "boolean",
    currentValue: false,
    choices: [],
  },
];

const CURSOR_FAST_SELECT = [
  PARAMETERIZED[0]!,
  PARAMETERIZED[1]!,
  {
    id: "fast",
    category: "model_config",
    name: "Fast",
    type: "select",
    currentValue: "false",
    choices: [
      { value: "false", name: "Off" },
      { value: "true", name: "Fast" },
    ],
  },
];

describe("liveSessionConfig", () => {
  it("parses sessionConfigOptions from snapshot JSON", () => {
    const options = parseLiveConfigOptions(PARAMETERIZED);
    expect(options).toHaveLength(3);
    expect(modelLiveOption(options!)).toMatchObject({ id: "model" });
  });

  it("encodes desired pin from live current values without bracket ids (#997)", () => {
    expect(encodeDesiredPinFromLiveOptions(PARAMETERIZED)).toBe(
      "grok-4.6|reasoning=high|fast=false",
    );
  });

  it("encodes operator selection using advertised option ids", () => {
    expect(
      encodeDesiredPinWithLiveSelection(PARAMETERIZED, {
        model: "composer-2.5",
        thoughtLevel: "low",
        fast: true,
      }),
    ).toBe("composer-2.5|reasoning=low|fast=true");
  });

  it("encodes Fast from true/false select currentValue (#1014)", () => {
    expect(encodeDesiredPinFromLiveOptions(CURSOR_FAST_SELECT)).toBe(
      "grok-4.6|reasoning=high|fast=false",
    );
    expect(
      encodeDesiredPinWithLiveSelection(CURSOR_FAST_SELECT, { fast: true }),
    ).toBe("grok-4.6|reasoning=high|fast=true");
  });

  it("rejects malformed option payloads", () => {
    expect(parseLiveConfigOptions([{ id: "model" }])).toBeUndefined();
  });
});
