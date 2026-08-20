import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import HarnessSwap from "./HarnessSwap";
import * as api from "@/shared/lib/api";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("HarnessSwap", () => {
  it("shows the current harness and stays collapsed until asked", () => {
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" />);
    expect(screen.getByTestId("harness-swap")).toHaveTextContent("Harness — Cursor");
    expect(screen.queryByRole("radio", { name: "Codex" })).not.toBeInTheDocument();
  });

  it("never renders a model picker", () => {
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" />);
    fireEvent.click(screen.getByTestId("harness-swap-open"));
    expect(screen.queryByText("Model")).not.toBeInTheDocument();
    expect(screen.queryByTestId("model-picker")).not.toBeInTheDocument();
  });

  it("switches with only the target harness", async () => {
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" onSwapped={vi.fn()} />);

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByRole("radio", { name: "Codex" }));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith("web/fix-login", "codex", undefined);
  });

  it("disables the current harness and refuses a same-harness apply", async () => {
    const spy = vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    render(<HarnessSwap handle="web/fix-login" currentAgent="cursor" />);

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    expect(screen.getByRole("radio", { name: "Cursor" })).toBeDisabled();
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    expect(await screen.findByTestId("harness-swap-error")).toHaveTextContent(
      "Same-harness model changes use in-session config chips",
    );
    expect(spy).not.toHaveBeenCalled();
  });
});
