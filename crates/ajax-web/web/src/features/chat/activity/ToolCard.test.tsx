import { describe, it, expect } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import ToolCard from "./ToolCard";
import { CONTENT_PREVIEW_LINES } from "./presentation";
import type { ToolCall } from "../session/public";

const call = (overrides: Partial<ToolCall> = {}): ToolCall => ({
  callId: "c1",
  title: "Read File",
  kind: "read",
  status: "completed",
  locations: ["/repo/crates/gateway/src/serve.rs"],
  content: [],
  ...overrides,
});

describe("ToolCard", () => {
  it("labels the row verb-first with the filename", () => {
    render(<ToolCard call={call()} />);
    expect(screen.getByRole("button", { name: /Read serve\.rs/i })).toBeInTheDocument();
  });

  it("distinguishes read and edit of the same path", () => {
    const path = "/repo/crates/gateway/src/serve.rs";
    const { unmount } = render(<ToolCard call={call({ kind: "read", locations: [path] })} />);
    expect(screen.getByRole("button", { name: /Read serve\.rs/i })).toBeInTheDocument();
    unmount();

    render(
      <ToolCard
        call={call({
          kind: "edit",
          title: "Edit File",
          locations: [path],
        })}
      />,
    );
    expect(screen.getByRole("button", { name: /Edited serve\.rs/i })).toBeInTheDocument();
  });

  it("does not auto-expand a completed read even with long content", () => {
    const lines = Array.from({ length: CONTENT_PREVIEW_LINES + 4 }, (_, i) => `line ${i + 1}`).join(
      "\n",
    );
    render(
      <ToolCard
        call={call({
          content: [{ type: "text", text: lines }],
        })}
      />,
    );

    expect(screen.getByRole("button", { name: /Read serve\.rs/i })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(screen.queryByTestId("session-tool-output")).not.toBeInTheDocument();
  });

  it("auto-expands a failed call and previews the tail of long output", () => {
    const head = Array.from({ length: 10 }, (_, i) => `setup ${i}`).join("\n");
    const tail = "assertion `left == right` failed\n  left: 1\n  right: 2";
    const text = `${head}\n${tail}`;
    render(
      <ToolCard
        call={call({
          kind: "execute",
          title: "cargo nextest",
          status: "failed",
          locations: [],
          content: [{ type: "text", text }],
        })}
      />,
    );

    expect(screen.getByTestId("session-tool-output")).toBeInTheDocument();
    expect(screen.getByTestId("session-tool-output")).toHaveTextContent("assertion");
    expect(screen.getByTestId("session-tool-output")).not.toHaveTextContent("setup 0");
    expect(screen.getByTestId("session-tool-output-expand")).toHaveTextContent("5 more lines");
    expect(screen.getByTestId("session-tool-failure-body")).toBeInTheDocument();
    expect(screen.getByTestId("session-tool-output")).toHaveAttribute("data-block-kind", "output");
  });

  it("tags search and read output blocks by tool kind", () => {
    const { unmount } = render(
      <ToolCard
        call={call({
          kind: "search",
          status: "in_progress",
          content: [{ type: "text", text: "src/main.rs\nsrc/lib.rs" }],
        })}
      />,
    );
    expect(screen.getByTestId("session-tool-output")).toHaveAttribute("data-block-kind", "search");
    unmount();

    render(
      <ToolCard
        call={call({
          status: "in_progress",
          content: [{ type: "text", text: "fn main() {}" }],
        })}
      />,
    );
    expect(screen.getByTestId("session-tool-output")).toHaveAttribute("data-block-kind", "read");
  });

  it("renders image and resource_link blocks in tool output", () => {
    render(
      <ToolCard
        call={call({
          status: "in_progress",
          content: [
            { type: "image", mimeType: "image/png", data: "aGVsbG8=" },
            {
              type: "resource_link",
              name: "README.md",
              uri: "file:///README.md",
            },
          ],
        })}
      />,
    );

    expect(screen.getByTestId("session-output-image")).toBeInTheDocument();
    expect(screen.getByTestId("session-output-resource-link")).toHaveTextContent("README.md");
  });

  it("previews the head for non-failed output and expands on demand", () => {
    const lines = Array.from({ length: CONTENT_PREVIEW_LINES + 3 }, (_, i) => `line ${i + 1}`).join(
      "\n",
    );
    render(
      <ToolCard
        call={call({
          kind: "execute",
          title: "cargo test",
          status: "in_progress",
          locations: [],
          content: [{ type: "text", text: lines }],
        })}
      />,
    );

    expect(screen.getByTestId("session-tool-output")).toHaveTextContent("line 1");
    expect(screen.getByTestId("session-tool-output")).not.toHaveTextContent(
      `line ${CONTENT_PREVIEW_LINES + 3}`,
    );

    fireEvent.click(screen.getByTestId("session-tool-output-expand"));
    expect(screen.getByTestId("session-tool-output")).toHaveTextContent(
      `line ${CONTENT_PREVIEW_LINES + 3}`,
    );
  });
});
