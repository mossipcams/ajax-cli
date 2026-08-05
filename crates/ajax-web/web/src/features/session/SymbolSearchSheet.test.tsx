import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import SymbolSearchSheet from "./SymbolSearchSheet";
import * as api from "@/shared/lib/api";

describe("SymbolSearchSheet", () => {
  beforeEach(() => {
    vi.spyOn(api, "fetchTaskSymbols").mockResolvedValue([
      {
        id: "src/session.rs:4:start_session",
        name: "start_session",
        kind: "method",
        path: "src/session.rs",
        startLine: 4,
        endLine: 6,
        preview: "pub fn start_session(&self) -> bool {",
        source: "pub fn start_session(&self) -> bool {\n    true\n}",
      },
    ]);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("searches, multi-selects, and confirms symbols", async () => {
    const onConfirm = vi.fn();
    render(
      <SymbolSearchSheet
        handle="web/fix-login"
        open
        selected={[]}
        onClose={vi.fn()}
        onConfirm={onConfirm}
      />,
    );

    fireEvent.change(screen.getByTestId("symbol-search-input"), {
      target: { value: "start_session" },
    });

    await waitFor(() => {
      expect(api.fetchTaskSymbols).toHaveBeenCalledWith("web/fix-login", "start_session");
    });

    fireEvent.click(screen.getByTestId("symbol-search-row-src/session.rs:4:start_session"));
    fireEvent.click(screen.getByTestId("symbol-search-confirm"));

    expect(onConfirm).toHaveBeenCalledWith([
      expect.objectContaining({ name: "start_session", kind: "method" }),
    ]);
  });
});
