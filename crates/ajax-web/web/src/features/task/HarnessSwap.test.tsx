import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import HarnessSwap from "./HarnessSwap";
import * as api from "@/shared/lib/api";

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
});
