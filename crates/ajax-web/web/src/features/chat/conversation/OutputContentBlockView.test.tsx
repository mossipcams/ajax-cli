import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import OutputContentBlockView from "./OutputContentBlockView";

describe("OutputContentBlockView", () => {
  it("renders an image from data uri", () => {
    render(
      <OutputContentBlockView
        block={{ type: "image", mimeType: "image/png", data: "aGVsbG8=" }}
      />,
    );
    expect(screen.getByRole("img")).toHaveAttribute("src", "data:image/png;base64,aGVsbG8=");
  });

  it("renders a resource link as name and uri", () => {
    render(
      <OutputContentBlockView
        block={{
          type: "resource_link",
          name: "README.md",
          uri: "file:///README.md",
        }}
      />,
    );
    expect(screen.getByTestId("session-output-resource-link")).toHaveTextContent("README.md");
    expect(screen.getByTestId("session-output-resource-link")).toHaveTextContent(
      "file:///README.md",
    );
  });
});
