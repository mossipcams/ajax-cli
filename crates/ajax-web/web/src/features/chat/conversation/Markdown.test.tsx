import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, screen, within } from "@testing-library/react";
import Markdown, { parseBlocks } from "./Markdown";

const markdownCss = readFileSync(
  join(import.meta.dirname, "../../../styles/chat/markdown.css"),
  "utf8",
);
const scrollingCss = readFileSync(
  join(import.meta.dirname, "../../../styles/chat/scrolling.css"),
  "utf8",
);

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
      { kind: "para", text: "one two" },
      { kind: "para", text: "three" },
    ]);
  });

  it("joins hard-wrapped lines with a single space without collapsing inline spacing", () => {
    expect(parseBlocks("SaySo trains   operators\non long turns.")).toEqual([
      { kind: "para", text: "SaySo trains   operators on long turns." },
    ]);
    expect(parseBlocks("Run `cargo test    --all` for\nall tests.")).toEqual([
      { kind: "para", text: "Run `cargo test    --all` for all tests." },
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
    const quote = screen.getByRole("blockquote");
    expect(quote).toHaveClass("md-quote");
    expect(within(quote).getByText("Quoted line")).toBeInTheDocument();
    const table = screen.getByRole("table");
    expect(table).toHaveClass("md-table");
    expect(within(table).getByRole("columnheader", { name: "Col A" })).toBeInTheDocument();
    expect(within(table).getByRole("cell", { name: "one" })).toBeInTheDocument();
    const [outerList, nestedList] = screen.getAllByRole("list");
    expect(within(outerList).getByText("parent")).toBeInTheDocument();
    expect(within(nestedList).getByText("child")).toBeInTheDocument();
    expect(outerList).toContainElement(nestedList);
  });

  it("does not link javascript or other non-http schemes", () => {
    render(<Markdown source="[bad](javascript:alert(1))" />);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.getByText(/\[bad\]/)).toBeInTheDocument();
  });

  it("renders hard-wrapped prose as one flowing paragraph", () => {
    render(<Markdown source={"First line of prose\nsecond line of prose"} />);
    expect(screen.getByText("First line of prose second line of prose")).toBeInTheDocument();
    expect(screen.getAllByRole("paragraph")).toHaveLength(1);
  });
});

describe("markdown layout css", () => {
  it("constrains prose and scroll containers inside the session thread column", () => {
    expect(markdownCss).toMatch(/\.md\s*\{[^}]*min-width:\s*0/);
    expect(markdownCss).toMatch(/\.md-para\s*\{[^}]*overflow-wrap:\s*anywhere/);
    expect(markdownCss).toMatch(/\.md-para\s*\{[^}]*white-space:\s*normal/);
    expect(markdownCss).toMatch(/\.md-block\s*\{[^}]*overflow-x:\s*auto/);
    expect(markdownCss).toMatch(/\.md-table-wrap\s*\{[^}]*overflow-x:\s*auto/);
    expect(markdownCss).toMatch(/\.md-table th,\s*\.md-table td\s*\{[^}]*overflow-wrap:\s*anywhere/);
    expect(markdownCss).not.toMatch(/\.md-table th,\s*\.md-table td\s*\{[^}]*white-space:\s*nowrap/);
    expect(markdownCss).toMatch(/\.md-block\s*\{[^}]*scrollbar-width:\s*thin/);
    expect(markdownCss).toMatch(/\.md-table-wrap\s*\{[^}]*scrollbar-width:\s*thin/);
    expect(scrollingCss).toMatch(/\.session-thread-inner\s*\{[^}]*min-width:\s*0/);
  });
});
