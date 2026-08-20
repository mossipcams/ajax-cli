import { describe, it, expect, vi, afterEach } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { SessionModelPickers, hasSessionModelControls } from "./SessionModelControls";

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

describe("SessionModelControls", () => {
  it("reports when live controls exist", () => {
    expect(hasSessionModelControls("codex", BRIDGE_OPTIONS)).toBe(true);
    expect(hasSessionModelControls("codex", [])).toBe(false);
    expect(hasSessionModelControls("cursor", BRIDGE_OPTIONS)).toBe(true);
  });

  it("renders nothing when no options are advertised", () => {
    const { container } = render(
      <SessionModelPickers agent="codex" options={[]} onApply={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("applies model changes immediately with advertised ids for bridge harnesses", () => {
    const onApply = vi.fn();
    render(
      <SessionModelPickers agent="codex" options={BRIDGE_OPTIONS} onApply={onApply} />,
    );
    const models = within(screen.getByTestId("session-config-model"));
    expect(models.getByRole("radio", { name: "Grok 4.6" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.click(models.getByRole("radio", { name: "Composer 2.5" }));
    expect(onApply).toHaveBeenCalledWith("model", "composer-2.5");
  });

  it("hides effort when only one level is advertised", () => {
    render(
      <SessionModelPickers
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
      <SessionModelPickers agent="codex" options={BRIDGE_OPTIONS} onApply={onApply} />,
    );
    fireEvent.pointerDown(screen.getByRole("radio", { name: "Low" }), { pointerType: "touch" });
    expect(onApply).toHaveBeenCalledWith("reasoning", "low");
    onApply.mockClear();
    fireEvent.click(screen.getByRole("radio", { name: "Low" }));
    expect(onApply).toHaveBeenCalledWith("reasoning", "low");
    fireEvent.click(within(screen.getByTestId("session-config-fast")).getByRole("radio", {
      name: "On",
    }));
    expect(onApply).toHaveBeenCalledWith("fast", true);
  });

  it("uses advertised model select for connected Cursor sessions", () => {
    const onApply = vi.fn();
    render(
      <SessionModelPickers
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

    fireEvent.click(
      within(screen.getByTestId("session-config-model")).getByRole("radio", {
        name: "Composer 2.5",
      }),
    );
    expect(onApply).toHaveBeenCalledWith("model", "composer-2.5");
    fireEvent.click(screen.getByRole("radio", { name: "High" }));
    expect(onApply).toHaveBeenCalledWith("reasoning", "high");
  });

  // Regression #1014: Cursor parameterized picker advertises Fast as true/false select.
  it("renders Fast as Off/On and applies advertised string values for Cursor", () => {
    const onApply = vi.fn();
    const { rerender } = render(
      <SessionModelPickers
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

    const fastOff = within(screen.getByTestId("session-config-fast"));
    expect(fastOff.getByRole("radio", { name: "Off" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(fastOff.getByRole("radio", { name: "On" }));
    expect(onApply).toHaveBeenCalledWith("fast", "true");

    rerender(
      <SessionModelPickers
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
    const fastOn = within(screen.getByTestId("session-config-fast"));
    expect(fastOn.getByRole("radio", { name: "On" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(fastOn.getByRole("radio", { name: "Off" }));
    expect(onApply).toHaveBeenCalledWith("fast", "false");
  });
});
