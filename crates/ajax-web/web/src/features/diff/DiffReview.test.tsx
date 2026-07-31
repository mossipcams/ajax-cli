import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import DiffReview from "./DiffReview";
import * as api from "@/shared/lib/api";

vi.mock("@/shared/lib/api", async () => {
  const actual = await vi.importActual<typeof import("@/shared/lib/api")>("@/shared/lib/api");
  return {
    ...actual,
    fetchTaskPullRequests: vi.fn(),
    fetchTaskDiff: vi.fn(),
  };
});

afterEach(() => {
  vi.restoreAllMocks();
});

beforeEach(() => {
  vi.mocked(api.fetchTaskPullRequests).mockResolvedValue([
    {
      number: 12,
      title: "Retry",
      url: "https://example.com/12",
      state: "OPEN",
      head_ref: "ajax/fix-login",
      head_sha: "abc",
    },
    {
      number: 9,
      title: "First",
      url: "https://example.com/9",
      state: "MERGED",
      head_ref: "ajax/fix-login",
      head_sha: "def",
    },
  ]);
  vi.mocked(api.fetchTaskDiff).mockResolvedValue({
    source: "pr:12",
    pr: {
      number: 12,
      title: "Retry",
      url: "https://example.com/12",
      state: "OPEN",
      head_ref: "ajax/fix-login",
      head_sha: "abc",
    },
    files: [
      {
        path: "src/a.ts",
        status: "modified",
        additions: 2,
        deletions: 1,
        hunks: [
          {
            header: "@@ -1,2 +1,3 @@",
            lines: [" context", "-old", "+new", "+more"],
          },
        ],
      },
    ],
  });
});

describe("DiffReview", () => {
  it("renders PR chips, file list, and opens hunks on file tap", async () => {
    render(<DiffReview handle="web/fix-login" title="Fix login" />);

    expect(await screen.findByTestId("diff-pr-strip")).toBeInTheDocument();
    expect(screen.getByTestId("diff-pr-12")).toHaveTextContent("#12 OPEN");
    expect(screen.getByTestId("diff-pr-9")).toHaveTextContent("#9 MERGED");
    expect(screen.getByTestId("diff-source")).toHaveTextContent("pr:12");
    expect(screen.getByTestId("diff-open-github")).toHaveAttribute(
      "href",
      "https://example.com/12",
    );

    fireEvent.click(screen.getByTestId("diff-file-row"));
    expect(screen.getByTestId("diff-hunk-viewer")).toBeInTheDocument();
    expect(screen.getByTestId("diff-hunk")).toHaveTextContent("+new");
  });

  it("shows empty local chip when no PRs exist", async () => {
    vi.mocked(api.fetchTaskPullRequests).mockResolvedValue([]);
    vi.mocked(api.fetchTaskDiff).mockResolvedValue({
      source: "local",
      pr: null,
      files: [],
    });

    render(<DiffReview handle="web/fix-login" />);
    expect(await screen.findByTestId("diff-pr-local")).toBeInTheDocument();
    expect(screen.getByTestId("diff-empty")).toHaveTextContent("No file changes");
  });

  it("surfaces load errors", async () => {
    vi.mocked(api.fetchTaskPullRequests).mockRejectedValue(new api.ApiError("http", "HTTP 502", 502));
    render(<DiffReview handle="web/fix-login" />);
    expect(await screen.findByTestId("diff-error")).toHaveTextContent("HTTP 502");
  });

  it("notifies parent when a PR chip is selected", async () => {
    const onSelectPr = vi.fn();
    render(<DiffReview handle="web/fix-login" onSelectPr={onSelectPr} />);
    fireEvent.click(await screen.findByTestId("diff-pr-9"));
    expect(onSelectPr).toHaveBeenCalledWith(9);
  });

  it("does not swipe-back when the gesture starts on a hunk", async () => {
    const onBack = vi.fn();
    render(<DiffReview handle="web/fix-login" onBack={onBack} />);
    fireEvent.click(await screen.findByTestId("diff-file-row"));
    const hunk = await screen.findByTestId("diff-hunk");
    fireEvent.touchStart(hunk, { changedTouches: [{ clientX: 40, clientY: 80 }] });
    fireEvent.touchMove(hunk, { changedTouches: [{ clientX: 140, clientY: 82 }] });
    fireEvent.touchEnd(hunk, { changedTouches: [{ clientX: 140, clientY: 82 }] });
    expect(onBack).not.toHaveBeenCalled();
  });
});
