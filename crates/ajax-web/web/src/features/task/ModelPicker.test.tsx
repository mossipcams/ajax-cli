import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
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
  it("shows a shortlist by default and reveals the full catalog on Show all (#948)", async () => {
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
      expect(screen.getAllByRole("radio").length).toBeLessThan(FULL_CATALOG.models.length);
    });
    expect(screen.getByRole("radio", { name: "Model 3" })).toHaveAttribute("aria-checked", "true");
    expect(screen.queryByRole("radio", { name: "Model 11" })).not.toBeInTheDocument();
    expect(screen.getByTestId("model-picker-toggle")).toHaveTextContent("Show all");

    fireEvent.click(screen.getByTestId("model-picker-toggle"));
    expect(screen.getAllByRole("radio")).toHaveLength(FULL_CATALOG.models.length);
    expect(screen.getByTestId("model-picker-toggle")).toHaveTextContent("Show fewer");
  });

  it("pins the current selection when it sits outside the shortlist", async () => {
    stubCatalog();
    render(
      <ModelPicker
        agent="codex"
        agentLabel="Codex"
        value="model-11"
        onChange={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(screen.getByRole("radio", { name: "Model 11" })).toHaveAttribute("aria-checked", "true");
    });
    expect(screen.getByRole("radio", { name: "Model 11" })).toBeInTheDocument();
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

  it("collapses Cursor Fast variants and emits composed catalog ids (#979)", async () => {
    const onChange = vi.fn();
    stubCatalog({
      models: [
        { id: "auto", label: "Auto" },
        { id: "composer-2.5", label: "Composer 2.5" },
        { id: "composer-2.5-fast", label: "Composer 2.5 Fast" },
        { id: "cursor-grok-4.6-high", label: "Grok 4.6" },
        { id: "cursor-grok-4.6-high-fast", label: "Grok 4.6 Fast" },
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
    expect(screen.queryByRole("radio", { name: /Composer 2.5 Fast/i })).not.toBeInTheDocument();
    expect(screen.getByTestId("model-fast")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Off" })).toHaveAttribute("aria-checked", "true");

    fireEvent.click(screen.getByRole("radio", { name: "On" }));
    expect(onChange).toHaveBeenCalledWith("cursor-grok-4.6-high-fast");

    onChange.mockClear();
    fireEvent.click(screen.getByRole("radio", { name: "Composer 2.5" }));
    expect(onChange).toHaveBeenCalledWith("composer-2.5");
  });
});
