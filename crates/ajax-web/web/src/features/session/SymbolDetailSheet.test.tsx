import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import SymbolDetailSheet from "./SymbolDetailSheet";
import type { WebSessionSymbolContext } from "./types";

const sampleSymbol: WebSessionSymbolContext = {
  id: "src/session.rs:2:start_session",
  name: "start_session",
  kind: "function",
  path: "src/session.rs",
  startLine: 2,
  endLine: 4,
  preview: "pub fn start_session() {",
  source: "pub fn start_session() {\n}\n",
};

describe("SymbolDetailSheet", () => {
  it("shows symbol details and attaches to the next message", () => {
    const onAttach = vi.fn();
    render(
      <SymbolDetailSheet
        symbol={sampleSymbol}
        open
        attached={[]}
        onClose={vi.fn()}
        onAttach={onAttach}
      />,
    );

    expect(screen.getByTestId("symbol-detail-meta")).toHaveTextContent(
      "function · src/session.rs",
    );
    expect(screen.getByTestId("symbol-detail-source")).toHaveTextContent(
      "pub fn start_session()",
    );

    fireEvent.click(screen.getByTestId("symbol-detail-attach"));
    expect(onAttach).toHaveBeenCalledWith(sampleSymbol);
  });

  it("disables attach when the symbol is already attached", () => {
    render(
      <SymbolDetailSheet
        symbol={sampleSymbol}
        open
        attached={[sampleSymbol]}
        onClose={vi.fn()}
        onAttach={vi.fn()}
      />,
    );

    expect(screen.getByTestId("symbol-detail-attach")).toBeDisabled();
    expect(screen.getByTestId("symbol-detail-attach")).toHaveTextContent("Attached");
  });
});
