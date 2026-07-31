import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen, within } from "@testing-library/react";
import RepoPanel from "./RepoPanel";
import type { RepoSummary } from "@/shared/lib/types";

const repos: RepoSummary[] = [
  {
    name: "web",
    path: "/Users/matt/projects/web",
    active_tasks: 2,
    attention_items: 1,
    reviewable_tasks: 3,
    cleanable_tasks: 0,
  },
  { name: "api", path: "/Users/matt/projects/api", active_tasks: 0, attention_items: 0 },
];

const rowFor = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}`) });

describe("RepoPanel", () => {
  it("renders one row per repo with its path", () => {
    render(<RepoPanel repos={repos} />);
    expect(within(rowFor("web")).getByText("/Users/matt/projects/web")).toBeInTheDocument();
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("shows only the counts the server reports as non-zero", () => {
    render(<RepoPanel repos={repos} />);
    const counts = screen.getByTestId("repo-counts-web");
    expect(counts).toHaveTextContent("1 need you");
    expect(counts).toHaveTextContent("3 to review");
    expect(counts).toHaveTextContent("2 active");
    // cleanable_tasks is 0 — a zero is not news.
    expect(counts).not.toHaveTextContent("clean up");
  });

  it("reads a repo with nothing outstanding as clear", () => {
    render(<RepoPanel repos={repos} />);
    expect(screen.queryByTestId("repo-counts-api")).toBeNull();
    expect(within(rowFor("api")).getByText("clear")).toBeInTheDocument();
  });

  it("never invents a count the server omitted", () => {
    render(<RepoPanel repos={[{ name: "docs" }]} />);
    expect(screen.queryByTestId("repo-counts-docs")).toBeNull();
    expect(screen.getByText("clear")).toBeInTheDocument();
  });

  it("selects a repo on tap and marks the active one", () => {
    const onSelectProject = vi.fn();
    render(<RepoPanel repos={repos} selectedProject="api" onSelectProject={onSelectProject} />);
    expect(rowFor("api")).toHaveAttribute("aria-current", "true");
    expect(rowFor("web")).not.toHaveAttribute("aria-current");

    fireEvent.click(rowFor("web"));
    expect(onSelectProject).toHaveBeenCalledWith("web");
  });

  it("renders nothing when there are no repos", () => {
    const { container } = render(<RepoPanel repos={[]} />);
    expect(container).toBeEmptyDOMElement();
  });
});
