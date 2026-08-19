import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import Markdown, { parseBlocks } from "./Markdown";

describe("parseBlocks", () => {
  it("keeps fenced code verbatim, including blank lines and markdown-looking text", () => {
    const blocks = parseBlocks("intro\n\n```rust\nlet x = 1;\n\n// - not a bullet\n```\nafter");
    expect(blocks).toEqual([
      { kind: "para", text: "intro" },
      { kind: "code", lang: "rust", text: "let x = 1;\n\n// - not a bullet" },
      { kind: "para", text: "after" },
    ]);
  });

  it("groups consecutive list items and splits on list type", () => {
    const blocks = parseBlocks("- one\n- two\n1. first\n2. second");
    expect(blocks).toEqual([
      {
        kind: "list",
        ordered: false,
        items: [
          { text: "one", children: [] },
          { text: "two", children: [] },
        ],
      },
      {
        kind: "list",
        ordered: true,
        items: [
          { text: "first", children: [] },
          { text: "second", children: [] },
        ],
      },
    ]);
  });

  it("treats an unterminated fence as code to the end rather than dropping it", () => {
    expect(parseBlocks("```\nhalf a block")).toEqual([
      { kind: "code", lang: "", text: "half a block" },
    ]);
  });

  it("joins wrapped lines into one paragraph and splits on a blank line", () => {
    expect(parseBlocks("one\ntwo\n\nthree")).toEqual([
      { kind: "para", text: "one\ntwo" },
      { kind: "para", text: "three" },
    ]);
  });
});

describe("Markdown", () => {
  it("renders inline code and bold without emitting markup as text", () => {
    render(<Markdown source="Call `reload()` when **ready**." />);
    expect(screen.getByText("reload()").tagName).toBe("CODE");
    expect(screen.getByText("ready").tagName).toBe("STRONG");
    expect(screen.queryByText(/\*\*/)).not.toBeInTheDocument();
  });

  it("renders a heading and a code block", () => {
    render(<Markdown source={"## Result\n\n```\nok\n```"} />);
    expect(screen.getByRole("heading", { name: "Result" }).tagName).toBe("H3");
    expect(screen.getByText("ok").tagName).toBe("CODE");
  });

  it("never interprets agent output as HTML", () => {
    render(<Markdown source={"<img src=x onerror=alert(1)>"} />);
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeInTheDocument();
  });

  it("renders http(s) links, blockquotes, tables, and nested lists", () => {
    render(
      <Markdown
        source={[
          "> Quoted line",
          "",
          "| Col A | Col B |",
          "| --- | --- |",
          "| one | two |",
          "",
          "- parent",
          "  - child",
          "",
          "See [docs](https://example.com/docs).",
        ].join("\n")}
      />,
    );
    expect(screen.getByRole("link", { name: "docs" })).toHaveAttribute(
      "href",
      "https://example.com/docs",
    );
    expect(screen.getByText("Quoted line").closest("blockquote")).toHaveClass("md-quote");
    expect(screen.getByRole("table")).toBeInTheDocument();
    expect(screen.getByText("child").closest("ul")?.parentElement?.closest("ul")).toBeTruthy();
  });

  it("does not link javascript or other non-http schemes", () => {
    render(<Markdown source="[bad](javascript:alert(1))" />);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.getByText(/\[bad\]/)).toBeInTheDocument();
  });
});
