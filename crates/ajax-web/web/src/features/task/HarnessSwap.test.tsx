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

function stubCatalog() {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        models: [
          { id: "gpt-5.6-sol[low]", label: "GPT-5.6-Sol (low)" },
          { id: "gpt-5.6-sol[high]", label: "GPT-5.6-Sol (high)" },
        ],
        default: "gpt-5.6-sol[low]",
      }),
    }),
  );
}

describe("HarnessSwap", () => {
  it("shows the current harness and stays collapsed until asked", () => {
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" />);
    expect(screen.getByTestId("harness-swap")).toHaveTextContent("Harness — Cursor");
    expect(screen.queryByRole("radio", { name: "Codex" })).not.toBeInTheDocument();
  });

  it("preselects the current harness and model when Switch opens", async () => {
    stubCatalog();
    render(
      <HarnessSwap
        handle="web/fix-login"
        currentAgent="cursor"
        currentModel="gpt-5.6-sol[low]"
      />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    expect(screen.getByRole("radio", { name: "Cursor" })).toHaveAttribute("aria-checked", "true");
    expect(await screen.findByRole("radio", { name: /GPT-5.6-Sol \(low\)/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("persists a model-only change on the same harness", async () => {
    stubCatalog();
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    render(
      <HarnessSwap handle="web/fix-login" currentAgent="cursor" currentModel="gpt-5.6-sol[low]" />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(await screen.findByRole("radio", { name: /GPT-5.6-Sol \(high\)/ }));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith("web/fix-login", "cursor", "gpt-5.6-sol[high]");
  });

  it("switches the task to the chosen harness and model", async () => {
    stubCatalog();
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    const onSwapped = vi.fn();
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" onSwapped={onSwapped} />);

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByRole("radio", { name: "Codex" }));
    fireEvent.click(await screen.findByRole("radio", { name: /GPT-5.6-Sol \(high\)/ }));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith("web/fix-login", "codex", "gpt-5.6-sol[high]");
    await waitFor(() => expect(onSwapped).toHaveBeenCalled());
  });

  it("keeps the panel open and shows why a swap was refused", async () => {
    stubCatalog();
    vi.spyOn(api, "swapTaskAgent").mockResolvedValue({
      ok: false,
      response: {},
      error: { message: "swapping harness needs a task Ajax started over ACP" },
    } as never);
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" />);

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    expect(await screen.findByTestId("harness-swap-error")).toHaveTextContent(
      "swapping harness needs a task Ajax started over ACP",
    );
    expect(screen.getByTestId("harness-swap-apply")).toBeInTheDocument();
  });

  it("submits pipe-form Cursor session_model from Switch (#979)", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "auto", label: "Auto" },
            { id: "composer-2.5", label: "Composer 2.5", hasFast: true },
          ],
          default: "composer-2.5",
        }),
      }),
    );
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    render(
      <HarnessSwap
        handle="web/fix-login"
        currentAgent="cursor"
        currentModel="composer-2.5|fast=false"
      />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(await screen.findByRole("radio", { name: "On" }));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith("web/fix-login", "cursor", "composer-2.5|fast=true");
  });

  it("seeds the live picker from sessionConfigOptions and applies the full pin without chip clicks", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "grok-4.6", label: "Grok 4.6", efforts: ["high", "low"], hasFast: true },
            { id: "composer-2.5", label: "Composer 2.5", hasFast: true },
          ],
          default: "grok-4.6",
        }),
      }),
    );
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    render(
      <HarnessSwap
        handle="web/fix-login"
        currentAgent="cursor"
        currentModel="composer-2.5"
        liveConfigOptions={LIVE_OPTIONS}
      />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    expect(await screen.findByRole("radio", { name: "High" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith(
      "web/fix-login",
      "cursor",
      "grok-4.6|reasoning=high|fast=false",
    );
  });

  it("keeps a live chip selection as the pending pin through Apply", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [
            { id: "grok-4.6", label: "Grok 4.6", efforts: ["high", "low"], hasFast: true },
            { id: "composer-2.5", label: "Composer 2.5", hasFast: true },
          ],
          default: "grok-4.6",
        }),
      }),
    );
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    render(
      <HarnessSwap
        handle="web/fix-login"
        currentAgent="cursor"
        currentModel="grok-4.6"
        liveConfigOptions={LIVE_OPTIONS}
      />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(await screen.findByRole("radio", { name: "Low" }));
    expect(screen.getByRole("radio", { name: "Low" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith(
      "web/fix-login",
      "cursor",
      "grok-4.6|reasoning=low|fast=false",
    );
  });
});
