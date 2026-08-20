import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import ModelPicker from "./ModelPicker";

const CURSOR_MODELS = {
  models: [
    { id: "auto", label: "Auto" },
    { id: "composer-2.5", label: "Composer 2.5" },
    { id: "composer-2.5-fast", label: "Composer 2.5 Fast" },
    { id: "cursor-grok-4.6-high", label: "Grok 4.6" },
    { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 Fast" },
  ],
  default: "cursor-grok-4.6-high",
};

const CODEX_OPTION_CATALOG = {
  agent: "codex",
  configOptions: [
    {
      id: "model",
      category: "model",
      name: "Model",
      type: "select",
      currentValue: "model-0",
      choices: Array.from({ length: 12 }, (_, index) => ({
        value: `model-${index}`,
        name: `Model ${index}`,
      })),
    },
  ],
};

function stubFetch({
  sessionModels = CURSOR_MODELS,
  optionCatalog = CODEX_OPTION_CATALOG,
}: {
  sessionModels?: unknown;
  optionCatalog?: unknown;
} = {}) {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation((url: string) => {
      if (url.includes("/api/session/models")) {
        return Promise.resolve({ ok: true, json: async () => sessionModels });
      }
      if (url.includes("/api/session/option-catalog")) {
        return Promise.resolve({ ok: true, json: async () => optionCatalog });
      }
      return Promise.resolve({ ok: false, status: 404 });
    }),
  );
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("ModelPicker", () => {
  it("lists exploded Cursor catalog ids grouped by family", async () => {
    stubFetch();
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="cursor-grok-4.6-high"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("group", { name: "Grok 4.6" })).toBeInTheDocument();
    });
    const grok = screen.getByRole("group", { name: "Grok 4.6" });
    expect(within(grok).getByRole("radio", { name: /Grok 4.6 High/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(within(grok).getByRole("radio", { name: "Grok 4.6 Fast" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Composer 2.5 Fast" })).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "Grok 4.6" })).not.toBeInTheDocument();
  });

  it("lists advertised model choices from the option catalog for other harnesses", async () => {
    stubFetch();
    render(
      <ModelPicker
        agent="codex"
        agentLabel="Codex"
        value="model-3"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getAllByRole("radio")).toHaveLength(12);
    });
    expect(screen.getByRole("radio", { name: "Model 3" })).toHaveAttribute("aria-checked", "true");
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
  });

  it("still decodes legacy exploded Cursor catalog ids (#979)", async () => {
    stubFetch();
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="cursor-grok-4.6-high"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: /Grok 4.6 High/ })).toHaveAttribute(
        "aria-checked",
        "true",
      );
    });
  });

  it("emits pipe storage on pick", async () => {
    const onChange = vi.fn();
    stubFetch();
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="cursor-grok-4.6-high"
        onChange={onChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "Composer 2.5 Fast" })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("radio", { name: "Composer 2.5 Fast" }));
    expect(onChange).toHaveBeenCalledWith("composer-2.5|fast=true");
  });

  it("emits pipe storage when clicking the Default row", async () => {
    const onChange = vi.fn();
    stubFetch();
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="composer-2.5"
        onChange={onChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: /Grok 4.6 High/ })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("radio", { name: /Grok 4.6 High/ }));
    expect(onChange).toHaveBeenCalledWith("grok-4.6|effort=high|fast=false");
    expect(onChange).not.toHaveBeenCalledWith("auto");
  });

  it("emits pipe storage when tapping the group header", async () => {
    const onChange = vi.fn();
    stubFetch({
      sessionModels: {
        models: [
          { id: "auto", label: "Auto" },
          { id: "cursor-grok-4.6-xhigh", label: "Grok 4.6 Extra High" },
          { id: "cursor-grok-4.6-high", label: "Grok 4.6 High" },
        ],
        default: "cursor-grok-4.6-high",
      },
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="cursor-grok-4.6-xhigh"
        onChange={onChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Grok 4.6" })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: "Grok 4.6" }));
    expect(onChange).toHaveBeenCalledWith("grok-4.6|effort=high");
  });

  it("uses family name only for group headers when labels include effort", async () => {
    stubFetch({
      sessionModels: {
        models: [
          { id: "auto", label: "Auto" },
          { id: "cursor-grok-4.6-xhigh", label: "Grok 4.6 Extra High" },
          { id: "cursor-grok-4.6-high", label: "Grok 4.6 High" },
        ],
        default: "cursor-grok-4.6-high",
      },
    });
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="cursor-grok-4.6-high"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("Grok 4.6")).toBeInTheDocument();
    });
    expect(screen.queryByText("Grok 4.6 Extra High", { selector: ".model-group-label" })).not.toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Grok 4.6 Extra High" })).toBeInTheDocument();
  });
});
