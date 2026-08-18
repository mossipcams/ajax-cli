import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
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
  // Regression for #948: a failed catalog fetch used to cache Auto plus the live
  // session model — exactly two buttons — instead of the harness catalog.
  it("lists every model the harness advertises (#948)", async () => {
    stubCatalog();
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="model-3"
        onChange={() => {}}
      />,
    );

    await waitFor(() => {
      expect(screen.getAllByRole("radio")).toHaveLength(FULL_CATALOG.models.length);
    });
    expect(screen.getByRole("radio", { name: "Model 3" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "Model 11" })).toBeInTheDocument();
  });

  it("does not treat a failed catalog fetch as Auto plus the live model (#948)", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 503 }));
    render(
      <ModelPicker
        agent="cursor"
        agentLabel="Cursor"
        value="composer-2.5"
        onChange={() => {}}
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
});
