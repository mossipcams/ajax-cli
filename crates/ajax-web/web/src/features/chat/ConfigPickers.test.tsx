import { describe, it, expect, vi, afterEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import ConfigPickers, { hasConfigPickerControls } from "./ConfigPickers";

const BRIDGE_OPTIONS = [
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

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("ConfigPickers", () => {
  it("reports when live controls exist", () => {
    expect(hasConfigPickerControls("codex", BRIDGE_OPTIONS)).toBe(true);
    expect(hasConfigPickerControls("codex", [])).toBe(false);
    expect(hasConfigPickerControls("cursor", BRIDGE_OPTIONS)).toBe(true);
  });

  it("renders nothing when no options are advertised", () => {
    const { container } = render(
      <ConfigPickers agent="codex" options={[]} onApply={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("applies model changes immediately with advertised ids for bridge harnesses", () => {
    const onApply = vi.fn();
    render(
      <ConfigPickers agent="codex" options={BRIDGE_OPTIONS} onApply={onApply} />,
    );
    fireEvent.change(screen.getByTestId("session-config-model"), {
      target: { value: "composer-2.5" },
    });
    expect(onApply).toHaveBeenCalledWith("model", "composer-2.5");
  });

  it("hides effort when only one level is advertised", () => {
    render(
      <ConfigPickers
        agent="codex"
        options={[
          BRIDGE_OPTIONS[0]!,
          {
            ...BRIDGE_OPTIONS[1]!,
            choices: [{ value: "high", name: "High" }],
          },
        ]}
        onApply={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("session-config-thought")).not.toBeInTheDocument();
  });

  it("applies thought level and Fast immediately for bridge harnesses", () => {
    const onApply = vi.fn();
    render(
      <ConfigPickers agent="codex" options={BRIDGE_OPTIONS} onApply={onApply} />,
    );
    fireEvent.click(screen.getByRole("radio", { name: "Low" }));
    expect(onApply).toHaveBeenCalledWith("reasoning", "low");
    fireEvent.click(screen.getByTestId("session-config-fast"));
    expect(onApply).toHaveBeenCalledWith("fast", true);
  });

  it("uses advertised model select for connected Cursor sessions", () => {
    const onApply = vi.fn();
    render(
      <ConfigPickers
        agent="cursor"
        confirmedModel="grok-4.6"
        options={[
          BRIDGE_OPTIONS[0]!,
          {
            ...BRIDGE_OPTIONS[1]!,
            currentValue: "xhigh",
            choices: [
              { value: "xhigh", name: "Extra High" },
              { value: "high", name: "High" },
            ],
          },
        ]}
        onApply={onApply}
      />,
    );

    fireEvent.change(screen.getByTestId("session-config-model"), {
      target: { value: "composer-2.5" },
    });
    expect(onApply).toHaveBeenCalledWith("model", "composer-2.5");
    fireEvent.click(screen.getByRole("radio", { name: "High" }));
    expect(onApply).toHaveBeenCalledWith("reasoning", "high");
  });

  // Regression #1014: Cursor parameterized picker advertises Fast as true/false select.
  it("renders Fast select and applies advertised string values for Cursor", () => {
    const onApply = vi.fn();
    const { rerender } = render(
      <ConfigPickers
        agent="cursor"
        confirmedModel="composer-2.5"
        options={[
          BRIDGE_OPTIONS[0]!,
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
        ]}
        onApply={onApply}
      />,
    );

    expect(screen.getByTestId("session-config-fast")).toBeInTheDocument();
    expect(screen.getByTestId("session-config-fast")).not.toBeChecked();
    fireEvent.click(screen.getByTestId("session-config-fast"));
    expect(onApply).toHaveBeenCalledWith("fast", "true");

    rerender(
      <ConfigPickers
        agent="cursor"
        confirmedModel="composer-2.5"
        options={[
          BRIDGE_OPTIONS[0]!,
          {
            id: "fast",
            category: "model_config",
            name: "Fast",
            type: "select",
            currentValue: "true",
            choices: [
              { value: "false", name: "Off" },
              { value: "true", name: "Fast" },
            ],
          },
        ]}
        onApply={onApply}
      />,
    );
    fireEvent.click(screen.getByTestId("session-config-fast"));
    expect(onApply).toHaveBeenCalledWith("fast", "false");
  });
});
