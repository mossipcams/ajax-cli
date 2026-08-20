import { describe, it, expect, vi, afterEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import ModelSwitchSheet, { modelControlLabel } from "./ModelSwitchSheet";

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
];

const CURSOR_MODELS = {
  models: [
    { id: "auto", label: "Auto" },
    { id: "composer-2.5", label: "Composer 2.5" },
    { id: "cursor-grok-4.6-high", label: "Grok 4.6 High" },
  ],
  default: "cursor-grok-4.6-high",
};

function stubCursorCatalog() {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation((url: string) => {
      if (url.includes("/api/session/models")) {
        return Promise.resolve({ ok: true, json: async () => CURSOR_MODELS });
      }
      return Promise.resolve({ ok: false, status: 404 });
    }),
  );
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("modelControlLabel", () => {
  it("prefers advertised choice names", () => {
    expect(modelControlLabel("grok-4.6", BRIDGE_OPTIONS)).toBe("Grok 4.6");
  });

  it("humanizes unknown catalog ids", () => {
    expect(modelControlLabel("cursor-grok-4.6-high", [])).toBe("grok 4.6 high");
  });
});

describe("ModelSwitchSheet", () => {
  it("renders nothing when closed", () => {
    const { container } = render(
      <ModelSwitchSheet
        open={false}
        onOpenChange={vi.fn()}
        panelId="model-panel"
        agent="codex"
        confirmedModel="grok-4.6"
        options={BRIDGE_OPTIONS}
        onApply={vi.fn()}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("shows bridge harness chips and closes from Close", async () => {
    const onOpenChange = vi.fn();
    render(
      <ModelSwitchSheet
        open
        onOpenChange={onOpenChange}
        panelId="model-panel"
        agent="codex"
        confirmedModel="grok-4.6"
        options={BRIDGE_OPTIONS}
        onApply={vi.fn()}
      />,
    );

    expect(screen.getByTestId("model-switch-sheet")).toBeInTheDocument();
    expect(screen.getByTestId("session-config-pickers")).toBeInTheDocument();
    expect(screen.getByTestId("session-config-model")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("model-switch-close"));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("applies Cursor model picks via advertised config options", async () => {
    const onApply = vi.fn();
    render(
      <ModelSwitchSheet
        open
        onOpenChange={vi.fn()}
        panelId="model-panel"
        agent="cursor"
        confirmedModel="grok-4.6"
        options={[
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
              { value: "xhigh", name: "Extra High" },
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
  });

  it("dismisses from backdrop tap", () => {
    const onOpenChange = vi.fn();
    render(
      <ModelSwitchSheet
        open
        onOpenChange={onOpenChange}
        panelId="model-panel"
        agent="codex"
        confirmedModel="grok-4.6"
        options={BRIDGE_OPTIONS}
        onApply={vi.fn()}
      />,
    );

    const scrim = screen.getByTestId("model-switch-sheet").parentElement!;
    fireEvent.click(scrim);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("does not dismiss when tapping a control inside the sheet", () => {
    const onOpenChange = vi.fn();
    render(
      <ModelSwitchSheet
        open
        onOpenChange={onOpenChange}
        panelId="model-panel"
        agent="cursor"
        confirmedModel="grok-4.6"
        options={BRIDGE_OPTIONS}
        onApply={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByTestId("session-config-model"), {
      target: { value: "composer-2.5" },
    });
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
