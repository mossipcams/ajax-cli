import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import HarnessSwap from "./HarnessSwap";
import * as api from "@/shared/lib/api";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";

const LIVE_OPTIONS: LiveSessionConfigOption[] = [
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
];

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function stubCatalog() {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation((url: string) => {
      if (url.includes("/api/session/models")) {
        return Promise.resolve({
          ok: true,
          json: async () => ({
            models: [
              { id: "auto", label: "Auto" },
              { id: "composer-2.5", label: "Composer 2.5" },
              { id: "cursor-grok-4.6-high", label: "Grok 4.6 High" },
            ],
            default: "composer-2.5",
          }),
        });
      }
      if (url.includes("/api/session/option-catalog")) {
        return Promise.resolve({
          ok: true,
          json: async () => ({
            agent: "codex",
            configOptions: [
              {
                id: "model",
                category: "model",
                name: "Model",
                type: "select",
                currentValue: "gpt-5.6-sol[low]",
                choices: [
                  { value: "gpt-5.6-sol[low]", name: "GPT-5.6-Sol (low)" },
                  { value: "gpt-5.6-sol[high]", name: "GPT-5.6-Sol (high)" },
                ],
              },
            ],
          }),
        });
      }
      return Promise.resolve({ ok: false, status: 404 });
    }),
  );
}

describe("HarnessSwap", () => {
  it("shows the current harness and stays collapsed until asked", () => {
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" />);
    expect(screen.getByTestId("harness-swap")).toHaveTextContent("Harness — Cursor");
    expect(screen.queryByRole("radio", { name: "Codex" })).not.toBeInTheDocument();
  });

  it("hides the model picker for a connected same-harness switch", async () => {
    render(
      <HarnessSwap
        handle="web/fix-login"
        currentAgent="cursor"
        currentModel="grok-4.6"
        liveConfigOptions={LIVE_OPTIONS}
      />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    expect(await screen.findByTestId("harness-swap-harness-only")).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: /Grok/i })).not.toBeInTheDocument();
  });

  it("switches the task to the chosen harness and model", async () => {
    stubCatalog();
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" onSwapped={vi.fn()} />);

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByRole("radio", { name: "Codex" }));
    fireEvent.click(await screen.findByRole("radio", { name: /GPT-5.6-Sol \(high\)/ }));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith("web/fix-login", "codex", "gpt-5.6-sol[high]");
  });

  it("refuses same-harness apply when connected", async () => {
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    render(
      <HarnessSwap
        handle="web/fix-login"
        currentAgent="cursor"
        liveConfigOptions={LIVE_OPTIONS}
      />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    expect(await screen.findByTestId("harness-swap-error")).toHaveTextContent(
      "Same-harness model changes use in-session config chips",
    );
    expect(spy).not.toHaveBeenCalled();
  });

  it("lists grouped Cursor catalog ids when switching to Cursor while connected", async () => {
    stubCatalog();
    render(
      <HarnessSwap
        handle="web/fix-login"
        currentAgent="codex"
        currentModel="gpt-5.6-sol[low]"
        liveConfigOptions={LIVE_OPTIONS}
      />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByRole("radio", { name: "Cursor" }));

    await waitFor(() => {
      expect(screen.getByText("Grok 4.6")).toBeInTheDocument();
    });
    expect(screen.getByRole("radio", { name: "Grok 4.6 High" })).toBeInTheDocument();
    expect(screen.queryByTestId("session-config-thought")).not.toBeInTheDocument();
    expect(screen.queryByTestId("session-config-fast")).not.toBeInTheDocument();
  });
});
