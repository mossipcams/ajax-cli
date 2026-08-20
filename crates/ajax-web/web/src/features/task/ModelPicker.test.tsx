import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import ModelPicker from "./ModelPicker";

const FULL_CATALOG = {
  models: Array.from({ length: 12 }, (_, index) => ({
    id: `model-${index}`,
    label: `Model ${index}`,
  })),
  default: "model-0",
};

function stubCatalog(catalog: unknown = FULL_CATALOG) {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({ ok: true, json: async () => catalog }),
  );
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("ModelPicker", () => {
  it("lists the full catalog without a shortlist or Show all toggle", async () => {
    stubCatalog();
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="model-3"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getAllByRole("radio")).toHaveLength(FULL_CATALOG.models.length);
    });
    expect(screen.getByRole("radio", { name: "Model 3" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "Model 11" })).toBeInTheDocument();
    expect(screen.queryByTestId("model-picker-toggle")).not.toBeInTheDocument();
  });

  it("does not treat a failed catalog fetch as Auto plus the live model (#948)", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 503 }));
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="composer-2.5"
        onChange={vi.fn()}
      />,
    );

    await waitFor(
      () => {
        expect(screen.getByTestId("model-catalog-error")).toHaveTextContent(
          "Could not read models from Cursor",
        );
      },
      { timeout: 3000 },
    );
    expect(screen.queryByRole("radio", { name: /composer/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /Auto/i })).not.toBeInTheDocument();
  });

  it("collapses Cursor Fast variants and emits pipe-form session_model (#979)", async () => {
    const onChange = vi.fn();
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "composer-2.5", label: "Composer 2.5", hasFast: true },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["high"], hasFast: true },
      ],
      default: "cursor-grok-4.6-high",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|effort=high|fast=false"
        onChange={onChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "Grok 4.6" })).toHaveAttribute("aria-checked", "true");
    });
    expect(screen.getByTestId("model-fast")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Off" })).toHaveAttribute("aria-checked", "true");

    fireEvent.click(screen.getByRole("radio", { name: "On" }));
    expect(onChange).toHaveBeenCalledWith("grok-4.6|effort=high|fast=true");

    onChange.mockClear();
    fireEvent.click(screen.getByRole("radio", { name: "Composer 2.5" }));
    expect(onChange).toHaveBeenCalledWith("composer-2.5|fast=false");
  });

  it("shows unknown current option when snapshot id base is not in catalog (#979)", async () => {
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["high"], hasFast: true },
      ],
      default: "cursor-grok-4.6-high",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="cursor-grok-5.0-high"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "cursor-grok-5.0-high" })).toHaveAttribute(
        "aria-checked",
        "true",
      );
    });
    expect(screen.queryByRole("radio", { name: "Grok 4.6" })).not.toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("decodes ACP bracket snapshot and exposes effort controls (#989)", async () => {
    const onChange = vi.fn();
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "gpt-5.6-sol", label: "GPT 5.6 Sol", efforts: ["medium", "high"], hasFast: true },
      ],
      default: "gpt-5.6-sol-high",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="gpt-5.6-sol[fast=false]"
        onChange={onChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "GPT 5.6 Sol" })).toHaveAttribute(
        "aria-checked",
        "true",
      );
    });
    expect(screen.getByTestId("model-effort")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "High" }));
    expect(onChange).toHaveBeenCalledWith("gpt-5.6-sol|effort=high|fast=false");
  });

  it("still decodes legacy exploded Cursor catalog ids (#979)", async () => {
    const onChange = vi.fn();
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["high"], hasFast: true },
      ],
      default: "cursor-grok-4.6-high",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="cursor-grok-4.6-high"
        onChange={onChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "Grok 4.6" })).toHaveAttribute("aria-checked", "true");
    });
    expect(screen.getByRole("radio", { name: "Off" })).toHaveAttribute("aria-checked", "true");
  });

  it("uses live sessionConfigOptions for effort and Fast while listing the full catalog (#997)", async () => {
    const onChange = vi.fn();
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["high"], hasFast: true },
        { id: "composer-2.5", label: "Composer 2.5", hasFast: true },
        { id: "extra-model", label: "Extra Model" },
      ],
      default: "grok-4.6",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|reasoning=high|fast=false"
        liveConfigOptions={[
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
        ]}
        onChange={onChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "Extra Model" })).toBeInTheDocument();
    });
    expect(screen.getByTestId("live-thought-level")).toBeInTheDocument();
    expect(screen.queryByTestId("model-effort")).not.toBeInTheDocument();
    expect(screen.getByTestId("live-model-fast")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Composer 2.5" }));
    expect(onChange).toHaveBeenCalledWith("composer-2.5|reasoning=high|fast=false");
  });

  it("hides the effort picker when only one thought level is advertised", async () => {
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["high"], hasFast: true },
      ],
      default: "grok-4.6",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|reasoning=high|fast=false"
        liveConfigOptions={[
          {
            id: "reasoning",
            category: "thought_level",
            name: "Effort",
            type: "select",
            currentValue: "high",
            choices: [{ value: "high", name: "High" }],
          },
        ]}
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "Grok 4.6" })).toBeInTheDocument();
    });
    expect(screen.queryByTestId("live-thought-level")).not.toBeInTheDocument();
    expect(screen.queryByTestId("model-effort")).not.toBeInTheDocument();
  });

  it("unions catalog Grok efforts when live thought_level advertises fewer choices (#1004)", async () => {
    const onChange = vi.fn();
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["low", "medium", "high", "xhigh"], hasFast: true },
      ],
      default: "grok-4.6",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|reasoning=high|fast=false"
        liveConfigOptions={[
          {
            id: "model",
            category: "model",
            name: "Model",
            type: "select",
            currentValue: "grok-4.6",
            choices: [{ value: "grok-4.6", name: "Grok 4.6" }],
          },
          {
            id: "reasoning",
            category: "thought_level",
            name: "Effort",
            type: "select",
            currentValue: "high",
            choices: [{ value: "high", name: "High" }],
          },
          {
            id: "fast",
            category: "model_config",
            name: "Fast",
            type: "boolean",
            currentValue: false,
            choices: [],
          },
        ]}
        onChange={onChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("live-thought-level")).toBeInTheDocument();
    });
    expect(screen.getByRole("radio", { name: "Low" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Medium" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Extra high" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "Low" }));
    expect(onChange).toHaveBeenCalledWith("grok-4.6|reasoning=low|fast=false");
  });

  it("shows catalog effort chips for Grok when disconnected (#1004)", async () => {
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["low", "medium", "high", "xhigh"], hasFast: true },
      ],
      default: "grok-4.6",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|effort=high|fast=false"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("model-effort")).toBeInTheDocument();
    });
    expect(screen.getByRole("radio", { name: "Low" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Extra high" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "High" })).toHaveAttribute("aria-checked", "true");
    expect(within(screen.getByTestId("model-fast")).getByRole("radio", { name: "Off" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("applies Effort and Fast on touch pointerdown so iOS cannot drop the click", async () => {
    const onChange = vi.fn();
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["low", "medium", "high", "xhigh"], hasFast: true },
      ],
      default: "grok-4.6",
    });
    const view = render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|effort=high|fast=false"
        onChange={onChange}
      />,
    );
    await waitFor(() => {
      expect(screen.getByTestId("model-effort")).toBeInTheDocument();
    });
    expect(within(screen.getByTestId("model-effort")).getByRole("radio", { name: "High" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(within(screen.getByTestId("model-fast")).getByRole("radio", { name: "Off" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.pointerDown(
      within(screen.getByTestId("model-effort")).getByRole("radio", { name: "Low" }),
      { pointerType: "touch" },
    );
    expect(onChange).toHaveBeenCalledWith("grok-4.6|effort=low|fast=false");
    onChange.mockClear();
    view.rerender(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|effort=low|fast=false"
        onChange={onChange}
      />,
    );
    expect(within(screen.getByTestId("model-effort")).getByRole("radio", { name: "Low" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.pointerDown(
      within(screen.getByTestId("model-fast")).getByRole("radio", { name: "On" }),
      { pointerType: "touch" },
    );
    expect(onChange).toHaveBeenCalledWith("grok-4.6|effort=low|fast=true");
  });

  it("does not reset Effort when the selected model row is tapped again", async () => {
    const onChange = vi.fn();
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["low", "medium", "high", "xhigh"], hasFast: true },
      ],
      default: "grok-4.6",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|effort=low|fast=true"
        onChange={onChange}
      />,
    );
    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "Grok 4.6" })).toHaveAttribute("aria-checked", "true");
    });
    fireEvent.click(screen.getByRole("radio", { name: "Grok 4.6" }));
    expect(onChange).not.toHaveBeenCalled();
    expect(within(screen.getByTestId("model-effort")).getByRole("radio", { name: "Low" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(within(screen.getByTestId("model-fast")).getByRole("radio", { name: "On" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("renders ACP-aligned catalog labels returned by GET /api/session/models", async () => {
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "grok-4.6", label: "Grok 4.6", efforts: ["high"], hasFast: true },
        { id: "gpt-5.6-sol", label: "GPT-5.6-Sol", efforts: ["high", "medium"], hasFast: true },
        {
          id: "claude-opus-5-thinking",
          label: "Claude Opus 5 Thinking",
          efforts: ["high"],
        },
      ],
      default: "grok-4.6",
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="grok-4.6|effort=high|fast=false"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "Grok 4.6" })).toBeInTheDocument();
    });
    expect(screen.getByRole("radio", { name: "GPT-5.6-Sol" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Claude Opus 5 Thinking" })).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /Cursor Grok/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /1M High/i })).not.toBeInTheDocument();
  });
});
