import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import {
  buildKnownSymbolIndex,
  mergeKnownSymbols,
  renderMessageContent,
  resolveKnownSymbol,
} from "./renderMessage";
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

const methodSymbol: WebSessionSymbolContext = {
  id: "src/session.rs:8:Session.start",
  name: "Session.start",
  kind: "method",
  path: "src/session.rs",
  startLine: 8,
  endLine: 10,
  preview: "fn start(&self) {",
  source: "fn start(&self) {\n}\n",
};

describe("renderMessage", () => {
  it("resolves only unique known symbol names", () => {
    const index = buildKnownSymbolIndex([sampleSymbol]);
    expect(resolveKnownSymbol(index, "start_session")).toEqual(sampleSymbol);
    expect(resolveKnownSymbol(index, "missing")).toBeNull();
  });

  it("merges known symbols by id", () => {
    const merged = mergeKnownSymbols([sampleSymbol], [methodSymbol, sampleSymbol]);
    expect(merged).toHaveLength(2);
    expect(merged.map((symbol) => symbol.id)).toEqual([sampleSymbol.id, methodSymbol.id]);
  });

  it("linkifies backtick and plain unique tokens", () => {
    const index = buildKnownSymbolIndex([sampleSymbol, methodSymbol]);
    const onSymbolClick = vi.fn();

    render(
      <p>
        {renderMessageContent(
          "Use `start_session` or Session.start in the handler.",
          index,
          onSymbolClick,
        )}
      </p>,
    );

    const backtickRef = screen.getByTestId(
      `ajax-web-session-symbol-ref-${sampleSymbol.id}`,
    );
    const plainRef = screen.getByTestId(`ajax-web-session-symbol-ref-${methodSymbol.id}`);
    expect(backtickRef).toHaveTextContent("start_session");
    expect(plainRef).toHaveTextContent("Session.start");

    fireEvent.click(backtickRef);
    expect(onSymbolClick).toHaveBeenCalledWith(sampleSymbol);
  });

  it("skips ambiguous plain tokens", () => {
    const duplicateA: WebSessionSymbolContext = { ...sampleSymbol, id: "a" };
    const duplicateB: WebSessionSymbolContext = { ...sampleSymbol, id: "b" };
    const index = buildKnownSymbolIndex([duplicateA, duplicateB]);
    const onSymbolClick = vi.fn();

    render(
      <p>{renderMessageContent("start_session is shared.", index, onSymbolClick)}</p>,
    );

    expect(
      screen.queryByTestId(`ajax-web-session-symbol-ref-${duplicateA.id}`),
    ).not.toBeInTheDocument();
  });
});
