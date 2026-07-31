import { useEffect, useMemo, useRef, useState, type TouchEvent } from "react";
import { fetchTaskDiff, fetchTaskPullRequests, ApiError } from "@/shared/lib/api";
import type { DiffFileView, PullRequestView, TaskDiffView } from "@/shared/lib/types";
import {
  isDiffPanGestureTarget,
  navigateSwipeEnd,
  navigateSwipeMove,
  navigateSwipeStart,
} from "@/shared/gestures/navigateSwipe";

interface Props {
  handle: string;
  title?: string | null;
  selectedPr?: number;
  onBack?: () => void;
  onSelectPr?: (pr: number | undefined) => void;
}

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | {
      status: "ready";
      prs: PullRequestView[];
      diff: TaskDiffView;
    };

function lineClass(line: string): string {
  if (line.startsWith("+") && !line.startsWith("+++")) return "diff-line add";
  if (line.startsWith("-") && !line.startsWith("---")) return "diff-line del";
  if (line.startsWith("@@")) return "diff-line hunk";
  return "diff-line";
}

function FileHunks({ file }: { file: DiffFileView }) {
  return (
    <article className="diff-file" data-testid="diff-file">
      <header className="diff-file-header">
        <span className="diff-file-path">{file.path}</span>
        <span className="diff-file-stats">
          <span className="add">+{file.additions}</span>
          <span className="del">−{file.deletions}</span>
        </span>
      </header>
      {file.hunks.length === 0 ? (
        <p className="diff-empty-hunks">No hunk content</p>
      ) : (
        file.hunks.map((hunk, index) => (
          <pre key={`${file.path}-${index}`} className="diff-hunk" data-testid="diff-hunk">
            <code>
              <span className="diff-line hunk">{hunk.header}</span>
              {hunk.lines.map((line, lineIndex) => (
                <span key={lineIndex} className={lineClass(line)}>
                  {line || " "}
                </span>
              ))}
            </code>
          </pre>
        ))
      )}
    </article>
  );
}

export default function DiffReview({
  handle,
  title,
  selectedPr,
  onBack,
  onSelectPr,
}: Props) {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const touchRef = useRef({ x: 0, y: 0, tracking: false, swipe: navigateSwipeStart() });

  useEffect(() => {
    let cancelled = false;
    setState({ status: "loading" });
    setSelectedPath(null);

    async function load() {
      try {
        let prs: Awaited<ReturnType<typeof fetchTaskPullRequests>> = [];
        try {
          prs = await fetchTaskPullRequests(handle);
        } catch {
          // Soft-fail: still try a local/selected diff projection.
          prs = [];
        }
        const pr = selectedPr ?? prs[0]?.number;
        const diff = await fetchTaskDiff(
          handle,
          pr !== undefined ? { pr } : { local: true },
        );
        if (!cancelled) setState({ status: "ready", prs, diff });
      } catch (error) {
        if (cancelled) return;
        const message =
          error instanceof ApiError
            ? error.message
            : error instanceof Error
              ? error.message
              : "Failed to load diff";
        setState({ status: "error", message });
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [handle, selectedPr]);

  const activeFile = useMemo(() => {
    if (state.status !== "ready" || !selectedPath) return null;
    return state.diff.files.find((file) => file.path === selectedPath) ?? null;
  }, [state, selectedPath]);

  function onTouchStart(event: TouchEvent) {
    if (isDiffPanGestureTarget(event.target)) {
      touchRef.current.tracking = false;
      return;
    }
    const touch = event.changedTouches[0];
    if (!touch) return;
    touchRef.current = {
      x: touch.clientX,
      y: touch.clientY,
      tracking: true,
      swipe: navigateSwipeStart(),
    };
  }

  function onTouchMove(event: TouchEvent) {
    if (!touchRef.current.tracking) return;
    const touch = event.changedTouches[0];
    if (!touch) return;
    const dx = touch.clientX - touchRef.current.x;
    const dy = touch.clientY - touchRef.current.y;
    touchRef.current.swipe = navigateSwipeMove(touchRef.current.swipe, dx, dy);
  }

  function onTouchEnd() {
    if (!touchRef.current.tracking) return;
    const direction = navigateSwipeEnd(touchRef.current.swipe);
    touchRef.current.tracking = false;
    if (direction === "right") onBack?.();
  }

  const heading = title || handle;
  const githubUrl =
    state.status === "ready" ? (state.diff.pr?.url ?? state.prs.find((p) => p.number === selectedPr)?.url) : null;

  return (
    <div
      className="diff-review"
      data-testid="diff-review"
      onTouchStart={onTouchStart}
      onTouchMove={onTouchMove}
      onTouchEnd={onTouchEnd}
      onTouchCancel={() => {
        touchRef.current.tracking = false;
      }}
    >
      <div className="detail-header" data-testid="diff-review-header">
        <button type="button" className="back" onClick={() => onBack?.()}>
          ← Back
        </button>
        <h1 className="detail-title">{heading}</h1>
        {githubUrl ? (
          <a
            className="diff-open-github"
            href={githubUrl}
            target="_blank"
            rel="noreferrer"
            data-testid="diff-open-github"
          >
            GitHub
          </a>
        ) : (
          <span className="interact-pill tone-idle">Diff</span>
        )}
      </div>

      {state.status === "loading" ? (
        <p className="diff-status" data-testid="diff-loading">
          Loading pull requests…
        </p>
      ) : null}

      {state.status === "error" ? (
        <p className="diff-status diff-error" data-testid="diff-error">
          {state.message}
        </p>
      ) : null}

      {state.status === "ready" ? (
        <>
          <div className="diff-pr-strip" data-testid="diff-pr-strip" role="tablist">
            {state.prs.length === 0 ? (
              <button
                type="button"
                className="diff-pr-chip is-active"
                data-testid="diff-pr-local"
                onClick={() => onSelectPr?.(undefined)}
              >
                Local
              </button>
            ) : (
              state.prs.map((pr) => {
                const active =
                  (selectedPr !== undefined && selectedPr === pr.number) ||
                  (selectedPr === undefined && state.diff.pr?.number === pr.number);
                return (
                  <button
                    key={pr.number}
                    type="button"
                    role="tab"
                    aria-selected={active}
                    className={`diff-pr-chip${active ? " is-active" : ""}`}
                    data-testid={`diff-pr-${pr.number}`}
                    onClick={() => onSelectPr?.(pr.number)}
                  >
                    #{pr.number} {pr.state}
                  </button>
                );
              })
            )}
          </div>

          <p className="diff-source" data-testid="diff-source">
            Source: {state.diff.source}
          </p>

          {activeFile ? (
            <div className="diff-hunk-viewer" data-testid="diff-hunk-viewer">
              <button
                type="button"
                className="diff-back-files"
                onClick={() => setSelectedPath(null)}
              >
                ← Files
              </button>
              <FileHunks file={activeFile} />
            </div>
          ) : state.diff.files.length === 0 ? (
            <p className="diff-status" data-testid="diff-empty">
              No file changes in this diff.
            </p>
          ) : (
            <ul className="diff-file-list" data-testid="diff-file-list">
              {state.diff.files.map((file) => (
                <li key={file.path}>
                  <button
                    type="button"
                    className="diff-file-row"
                    data-testid="diff-file-row"
                    onClick={() => setSelectedPath(file.path)}
                  >
                    <span className="diff-file-path">{file.path}</span>
                    <span className="diff-file-stats">
                      <span className="add">+{file.additions}</span>
                      <span className="del">−{file.deletions}</span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      ) : null}
    </div>
  );
}
