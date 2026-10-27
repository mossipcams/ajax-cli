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

  it("recognises fences indented up to three spaces, closing them the same way", () => {
    expect(parseBlocks("Try:\n   ```js\nlet x = 1;\n   ```\nafter")).toEqual([
      { kind: "para", text: "Try:" },
      { kind: "code", lang: "js", text: "let x = 1;" },
      { kind: "para", text: "after" },
    ]);
  });

  it("accepts any info string on a fence opener, not just word characters", () => {
    expect(parseBlocks("```c++\nint x;\n```")).toEqual([
      { kind: "code", lang: "c++", text: "int x;" },
    ]);
  });

  it("does not open a fence on an inline ``` span, so following lines stay prose", () => {
    expect(parseBlocks("```inline``` then text\nmore lines\n\nafter")).toEqual([
      { kind: "para", text: "```inline``` then text more lines" },
      { kind: "para", text: "after" },
    ]);
  });

  it("takes the first word of a fence info string as the language", () => {
    expect(parseBlocks("```js title=x\nlet x = 1;\n```")).toEqual([
      { kind: "code", lang: "js", text: "let x = 1;" },
    ]);
  });

  it("keeps the first number of an ordered list so it can render with start", () => {
    expect(parseBlocks("3. third\n4. fourth")).toEqual([
      {
        kind: "list",
        ordered: true,
        start: 3,
        items: [
          { text: "third", children: [] },
          { text: "fourth", children: [] },
        ],
      },
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

  it("links bare http(s) URLs but leaves trailing punctuation outside the link", () => {
    render(
      <Markdown
        source={"Docs at https://example.com/docs. Also (https://example.org/a), then http://example.net,"}
      />,
    );
    const docs = screen.getByRole("link", { name: "https://example.com/docs" });
    expect(docs).toHaveAttribute("href", "https://example.com/docs");
    const org = screen.getByRole("link", { name: "https://example.org/a" });
    expect(org).toHaveAttribute("href", "https://example.org/a");
    const net = screen.getByRole("link", { name: "http://example.net" });
    expect(net).toHaveAttribute("href", "http://example.net");
    expect(screen.getAllByRole("link")).toHaveLength(3);
  });

  it("starts ordered lists at their first number and omits start for 1", () => {
    const { unmount } = render(<Markdown source={"3. third\n4. fourth"} />);
    expect(screen.getByRole("list")).toHaveAttribute("start", "3");
    unmount();
    render(<Markdown source={"1. first\n2. second"} />);
    expect(screen.getByRole("list")).not.toHaveAttribute("start");
  });

  it("keeps a balanced closing parenthesis in a bare URL and leaves an unbalanced one outside", () => {
    render(
      <Markdown
        source={"See https://en.wikipedia.org/wiki/Rust_(programming_language) or (https://example.com/a) here"}
      />,
    );
    const rust = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
    expect(screen.getByRole("link", { name: rust })).toHaveAttribute("href", rust);
    expect(screen.getByRole("link", { name: "https://example.com/a" })).toHaveAttribute(
      "href",
      "https://example.com/a",
    );
    expect(screen.getAllByRole("link")).toHaveLength(2);
  });

  it("keeps an unclosed opening parenthesis inside a bare URL", () => {
    render(<Markdown source={"Read https://x.test/a(b today"} />);
    expect(screen.getByRole("link", { name: "https://x.test/a(b" })).toHaveAttribute(
      "href",
      "https://x.test/a(b",
    );
  });

  it("keeps links and code inside emphasis", () => {
    render(<Markdown source={"*see [the docs](https://example.com/docs) and `npm test`*"} />);
    const emphasis = screen.getByText((_, element) => element?.tagName === "EM");
    expect(within(emphasis).getByRole("link", { name: "the docs" })).toHaveAttribute(
      "href",
      "https://example.com/docs",
    );
    expect(within(emphasis).getByText("npm test").tagName).toBe("CODE");
  });

  it("renders *text* and _text_ as emphasis without touching bold, code, snake_case, or bullets", () => {
    render(
      <Markdown
        source={"*this* and _that_, plus **bold**, `code`, snake_case_name and 2 * 3 * 4"}
      />,
    );
    expect(screen.getByText("this").tagName).toBe("EM");
    expect(screen.getByText("that").tagName).toBe("EM");
    expect(screen.getByText("bold").tagName).toBe("STRONG");
    expect(screen.getByText("code").tagName).toBe("CODE");
    // The paragraph's own text keeps "snake_case_name" and "2 * 3 * 4" intact, so neither was split into <em>.
    expect(screen.getByText(/snake_case_name and 2 \* 3 \* 4/)).toBeInTheDocument();
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    render(<Markdown source={"* item one"} />);
    expect(screen.getByRole("list")).toHaveTextContent("item one");
  });

  it("keeps bare javascript: URLs and raw HTML as inert text", () => {
    render(<Markdown source={"Run javascript:alert(1) or <img src=x onerror=alert(1)>"} />);
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
    expect(screen.getByText(/javascript:alert\(1\) or <img src=x onerror=alert\(1\)>/)).toBeInTheDocument();
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
