import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import DiffReview from "./DiffReview";
import * as api from "@/shared/lib/api";
import type { DiffFileView, DiffJudgmentView, TaskDiffView } from "@/shared/lib/types";
import { SWIPE_PAGE_COMMIT_MS } from "@/shared/hooks/useSwipePageTransition";

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

function judgmentFor(files: DiffFileView[], flags: DiffJudgmentView["flags"] = []): DiffJudgmentView {
  const signal = files.filter((file) => file.role === "signal");
  const noise = files.filter((file) => file.role === "noise");
  const reading_order = [...signal]
    .sort((left, right) => {
      const churn =
        right.additions + right.deletions - (left.additions + left.deletions);
      if (churn !== 0) return churn;
      return left.path.localeCompare(right.path);
    })
    .map((file) => file.path);
  return {
    totals: {
      files: files.length,
      signal: signal.length,
      noise: noise.length,
      additions: files.reduce((sum, file) => sum + file.additions, 0),
      deletions: files.reduce((sum, file) => sum + file.deletions, 0),
    },
    reading_order,
    flags,
  };
}

function diffView(
  partial: Omit<TaskDiffView, "judgment"> & { judgment?: DiffJudgmentView },
): TaskDiffView {
  return {
    ...partial,
    judgment: partial.judgment ?? judgmentFor(partial.files),
  };
}

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
  const files: DiffFileView[] = [
    {
      path: "src/a.ts",
      status: "modified",
      role: "signal",
      additions: 2,
      deletions: 1,
      hunks: [
        {
          header: "@@ -1,2 +1,3 @@",
          lines: [" context", "-old", "+new", "+more"],
        },
      ],
    },
  ];
  vi.mocked(api.fetchTaskDiff).mockResolvedValue(
    diffView({
      source: "pr:12",
      pr: {
        number: 12,
        title: "Retry",
        url: "https://example.com/12",
        state: "OPEN",
        head_ref: "ajax/fix-login",
        head_sha: "abc",
      },
      files,
      judgment: judgmentFor(files, [
        {
          kind: "unexpected_path",
          severity: "info",
          path: "src/a.ts",
          detail: "unexpected path outside common roots",
        },
      ]),
    }),
  );
});

describe("DiffReview", () => {
  it("renders PR chips, auto-opens top signal file, and shows hunks", async () => {
    render(<DiffReview handle="web/fix-login" title="Fix login" />);

    expect(await screen.findByTestId("diff-pr-strip")).toBeInTheDocument();
    expect(screen.getByTestId("diff-pr-12")).toHaveTextContent("#12 OPEN");
    expect(screen.getByTestId("diff-pr-9")).toHaveTextContent("#9 MERGED");
    expect(screen.getByTestId("diff-source")).toHaveTextContent("pr:12");
    expect(screen.getByTestId("diff-open-github")).toHaveAttribute(
      "href",
      "https://example.com/12",
    );

    expect(await screen.findByTestId("diff-hunk-viewer")).toBeInTheDocument();
    expect(screen.getByTestId("diff-hunk")).toHaveTextContent("+new");
  });

  it("renders orientation, flags, and guide chips from judgment", async () => {
    render(<DiffReview handle="web/fix-login" title="Fix login" />);

    expect(await screen.findByTestId("diff-orientation")).toHaveTextContent(
      "1 files · 1 signal · 0 noise · +2 −1",
    );
    expect(screen.getByTestId("diff-flags")).toBeInTheDocument();
    expect(screen.getByTestId("diff-flag")).toHaveAttribute("data-flag-kind", "unexpected_path");
    expect(screen.getByTestId("diff-guide-strip")).toBeInTheDocument();
    expect(screen.getByTestId("diff-guide-chip")).toHaveTextContent("a.ts");

    expect(await screen.findByTestId("diff-hunk-viewer")).toBeInTheDocument();
    fireEvent.click(screen.getByText("← Files"));
    fireEvent.click(screen.getByTestId("diff-flag"));
    expect(await screen.findByTestId("diff-hunk-viewer")).toBeInTheDocument();
  });

  it("shows empty local chip when no PRs exist", async () => {
    vi.mocked(api.fetchTaskPullRequests).mockResolvedValue([]);
    vi.mocked(api.fetchTaskDiff).mockResolvedValue(
      diffView({
        source: "local",
        pr: null,
        files: [],
      }),
    );

    render(<DiffReview handle="web/fix-login" />);
    expect(await screen.findByTestId("diff-pr-local")).toBeInTheDocument();
    expect(screen.getByTestId("diff-empty")).toHaveTextContent("No file changes");
    expect(screen.getByTestId("diff-orientation")).toHaveTextContent(
      "0 files · 0 signal · 0 noise · +0 −0",
    );
  });

  it("surfaces load errors", async () => {
    vi.mocked(api.fetchTaskPullRequests).mockRejectedValue(new api.ApiError("http", "gh down", 502));
    vi.mocked(api.fetchTaskDiff).mockRejectedValue(new api.ApiError("http", "git failed", 502));
    render(<DiffReview handle="web/fix-login" />);
    expect(await screen.findByTestId("diff-error")).toHaveTextContent("git failed");
  });

  it("still loads a local diff when PR list fetch fails", async () => {
    vi.mocked(api.fetchTaskPullRequests).mockRejectedValue(new api.ApiError("http", "gh down", 502));
    vi.mocked(api.fetchTaskDiff).mockResolvedValue(
      diffView({
        source: "local",
        pr: null,
        files: [],
      }),
    );
    render(<DiffReview handle="web/fix-login" />);
    expect(await screen.findByTestId("diff-pr-local")).toBeInTheDocument();
    expect(screen.getByTestId("diff-empty")).toHaveTextContent("No file changes");
    expect(api.fetchTaskDiff).toHaveBeenCalledWith("web/fix-login", { local: true });
  });

  it("requests the first listed PR when selectedPr is unset", async () => {
    render(<DiffReview handle="web/fix-login" />);
    await screen.findByTestId("diff-pr-strip");
    expect(api.fetchTaskDiff).toHaveBeenCalledWith("web/fix-login", { pr: 12 });
  });

  it("ignores stale diff responses when selectedPr changes quickly", async () => {
    const pr9Diff = {
      source: "pr:9",
      pr: {
        number: 9,
        title: "First",
        url: "https://example.com/9",
        state: "MERGED",
        head_ref: "ajax/fix-login",
        head_sha: "def",
      },
      files: [
        {
          path: "src/fresh.ts",
          status: "modified",
          role: "signal",
          additions: 1,
          deletions: 0,
          hunks: [{ header: "@@", lines: ["+fresh"] }],
        },
      ],
    };
    const pr12Diff = {
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
          path: "src/stale.ts",
          status: "modified",
          role: "signal",
          additions: 1,
          deletions: 0,
          hunks: [{ header: "@@", lines: ["+stale"] }],
        },
      ],
    };

    const resolvers: Array<(value: typeof pr9Diff) => void> = [];
    vi.mocked(api.fetchTaskDiff).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolvers.push(resolve);
        }),
    );

    const { rerender } = render(<DiffReview handle="web/fix-login" selectedPr={12} />);
    await waitFor(() => expect(resolvers).toHaveLength(1));

    rerender(<DiffReview handle="web/fix-login" selectedPr={9} />);
    await waitFor(() => expect(resolvers).toHaveLength(2));

    await act(async () => {
      resolvers[1](pr9Diff);
    });
    expect(await screen.findByTestId("diff-source")).toHaveTextContent("pr:9");
    expect(screen.getByTestId("diff-file")).toHaveTextContent("src/fresh.ts");

    await act(async () => {
      resolvers[0](pr12Diff);
    });
    expect(screen.getByTestId("diff-source")).toHaveTextContent("pr:9");
    expect(screen.getByTestId("diff-file")).toHaveTextContent("src/fresh.ts");
  });

  it("shows a fallback banner when PR patch fell back to local diff", async () => {
    vi.mocked(api.fetchTaskDiff).mockResolvedValue({
      source: "local",
      pr: null,
      fell_back_from_pr: 12,
      files: [
        {
          path: "src/a.ts",
          status: "modified",
          role: "signal",
          additions: 1,
          deletions: 0,
          hunks: [{ header: "@@", lines: ["+new"] }],
        },
      ],
    });

    render(<DiffReview handle="web/fix-login" selectedPr={12} />);

    const banner = await screen.findByTestId("diff-fallback-banner");
    expect(banner).toHaveTextContent("PR #12 patch unavailable");
    expect(banner).toHaveTextContent("local");
    expect(screen.getByTestId("diff-source")).toHaveTextContent("local");
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
    const hunk = await screen.findByTestId("diff-hunk");
    fireEvent.touchStart(hunk, { changedTouches: [{ clientX: 40, clientY: 80 }] });
    fireEvent.touchMove(hunk, { changedTouches: [{ clientX: 140, clientY: 82 }] });
    fireEvent.touchEnd(hunk, { changedTouches: [{ clientX: 140, clientY: 82 }] });
    expect(onBack).not.toHaveBeenCalled();
  });

  it("does not swipe-back on a left swipe", async () => {
    const onBack = vi.fn();
    render(<DiffReview handle="web/fix-login" onBack={onBack} />);
    const root = await screen.findByTestId("diff-review");
    fireEvent.touchStart(root, { changedTouches: [{ clientX: 200, clientY: 80 }] });
    fireEvent.touchMove(root, { changedTouches: [{ clientX: 120, clientY: 80 }] });
    fireEvent.touchEnd(root, { changedTouches: [{ clientX: 120, clientY: 80 }] });
    expect(onBack).not.toHaveBeenCalled();
  });

  it("swipe-backs on a right swipe", async () => {
    const onBack = vi.fn();
    render(<DiffReview handle="web/fix-login" onBack={onBack} />);
    const root = await screen.findByTestId("diff-review");
    vi.useFakeTimers();
    try {
      Object.defineProperty(root, "clientWidth", { value: 390, configurable: true });
      fireEvent.touchStart(root, { changedTouches: [{ clientX: 40, clientY: 80 }] });
      fireEvent.touchMove(root, { changedTouches: [{ clientX: 140, clientY: 82 }] });
      fireEvent.touchEnd(root, { changedTouches: [{ clientX: 140, clientY: 82 }] });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(SWIPE_PAGE_COMMIT_MS + 50);
      });
      expect(onBack).toHaveBeenCalledOnce();
    } finally {
      vi.useRealTimers();
    }
  });

  it("lists signal files first, collapses noise, and opens top signal by churn", async () => {
    const files: DiffFileView[] = [
      {
        path: "Cargo.lock",
        status: "modified",
        role: "noise",
        additions: 100,
        deletions: 50,
        hunks: [{ header: "@@", lines: ["+lock"] }],
      },
      {
        path: "src/b.ts",
        status: "modified",
        role: "signal",
        additions: 1,
        deletions: 0,
        hunks: [{ header: "@@", lines: ["+b"] }],
      },
      {
        path: "src/a.ts",
        status: "modified",
        role: "signal",
        additions: 5,
        deletions: 2,
        hunks: [{ header: "@@", lines: ["+a"] }],
      },
    ];
    vi.mocked(api.fetchTaskDiff).mockResolvedValue(
      diffView({
        source: "local",
        pr: null,
        files,
      }),
    );

    render(<DiffReview handle="web/fix-login" />);

    expect(await screen.findByTestId("diff-hunk-viewer")).toBeInTheDocument();
    expect(screen.getByTestId("diff-file")).toHaveTextContent("src/a.ts");

    fireEvent.click(screen.getByText("← Files"));
    const rows = screen.getAllByTestId("diff-file-row");
    expect(rows[0]).toHaveTextContent("src/a.ts");
    expect(rows[1]).toHaveTextContent("src/b.ts");
    expect(screen.queryByText("Cargo.lock")).not.toBeInTheDocument();
    expect(screen.getByTestId("diff-noise-toggle")).toHaveTextContent("1 noise");

    fireEvent.click(screen.getByTestId("diff-noise-toggle"));
    expect(screen.getByText("Cargo.lock")).toBeInTheDocument();
  });

  it("stays on collapsed file list when only noise files exist", async () => {
    const files: DiffFileView[] = [
      {
        path: "Cargo.lock",
        status: "modified",
        role: "noise",
        additions: 2,
        deletions: 1,
        hunks: [{ header: "@@", lines: ["+lock"] }],
      },
    ];
    vi.mocked(api.fetchTaskDiff).mockResolvedValue(
      diffView({
        source: "local",
        pr: null,
        files,
      }),
    );

    render(<DiffReview handle="web/fix-login" />);

    expect(await screen.findByTestId("diff-file-list")).toBeInTheDocument();
    expect(screen.queryByTestId("diff-hunk-viewer")).not.toBeInTheDocument();
    expect(screen.getByTestId("diff-noise-toggle")).toHaveTextContent("1 noise");
    expect(screen.queryByTestId("diff-file-row")).not.toBeInTheDocument();
    expect(screen.queryByTestId("diff-guide-strip")).not.toBeInTheDocument();
  });
});
