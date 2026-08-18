import { useEffect, useMemo, useRef, useState } from "react";
import type {
  DiffFileView,
  DiffFlagKind,
  DiffFlagView,
} from "@/shared/lib/types";
import { isDiffPanGestureTarget } from "@/shared/gestures/navigateSwipe";
import { useSwipePageTransition } from "@/shared/hooks/useSwipePageTransition";
import { useTaskDiffReviewQueries } from "./useTaskDiffReviewQueries";

interface Props {
  handle: string;
  title?: string | null;
  selectedPr?: number;
  onBack?: () => void;
  onSelectPr?: (pr: number | undefined) => void;
}

const FLAG_DETAIL: Record<DiffFlagKind, string> = {
  unexpected_path: "unexpected path outside common roots",
  deleted_test: "deleted test file",
  secret_pattern: "possible secret in added line",
  permission_widen: "permission widening in added line",
  dependency_manifest: "dependency manifest changed",
  deleted_check_path: "deleted check or workflow path",
};

function lineClass(line: string): string {
  if (line.startsWith("+") && !line.startsWith("+++")) return "diff-line add";
  if (line.startsWith("-") && !line.startsWith("---")) return "diff-line del";
  if (line.startsWith("@@")) return "diff-line hunk";
  return "diff-line";
}

function sortByChurn(files: DiffFileView[]): DiffFileView[] {
  return [...files].sort((left, right) => {
    const churnDiff =
      right.additions + right.deletions - (left.additions + left.deletions);
    if (churnDiff !== 0) return churnDiff;
    return left.path.localeCompare(right.path);
  });
}

function partitionFilesByRole(files: DiffFileView[], readingOrder: string[]) {
  const byPath = new Map(files.map((file) => [file.path, file]));
  const signalFiles = readingOrder
    .map((path) => byPath.get(path))
    .filter((file): file is DiffFileView => Boolean(file));
  const noiseFiles = sortByChurn(files.filter((file) => file.role === "noise"));
  return { signalFiles, noiseFiles };
}

const GUIDE_CHIP_LIMIT = 5;

function FileRow({
  file,
  onSelect,
}: {
  file: DiffFileView;
  onSelect: () => void;
}) {
  return (
    <li>
      <button
        type="button"
        className="diff-file-row"
        data-testid="diff-file-row"
        data-file-role={file.role}
        onClick={onSelect}
      >
        <span className="diff-file-path">{file.path}</span>
        <span className="diff-file-stats">
          <span className="add">+{file.additions}</span>
          <span className="del">−{file.deletions}</span>
        </span>
      </button>
    </li>
  );
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

function JudgmentFlags({
  flags,
  onSelectPath,
}: {
  flags: DiffFlagView[];
  onSelectPath: (path: string) => void;
}) {
  if (flags.length === 0) return null;
  return (
    <ul className="diff-flags" data-testid="diff-flags">
      {flags.map((flag, index) => (
        <li key={`${flag.kind}-${flag.path}-${index}`}>
          <button
            type="button"
            className={`diff-flag severity-${flag.severity} is-action`}
            data-testid="diff-flag"
            data-flag-kind={flag.kind}
            data-flag-severity={flag.severity}
            onClick={() => onSelectPath(flag.path)}
          >
            <span className="diff-flag-kind">{flag.kind}</span>
            <span className="diff-flag-detail">{FLAG_DETAIL[flag.kind]}</span>
            <span className="diff-flag-path">{flag.path}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}

function GuideStrip({
  paths,
  onSelectPath,
}: {
  paths: string[];
  onSelectPath: (path: string) => void;
}) {
  if (paths.length === 0) return null;
  return (
    <div className="diff-guide-strip" data-testid="diff-guide-strip" role="navigation">
      {paths.slice(0, GUIDE_CHIP_LIMIT).map((path) => {
        const label = path.includes("/") ? path.slice(path.lastIndexOf("/") + 1) : path;
        return (
          <button
            key={path}
            type="button"
            className="diff-guide-chip"
            data-testid="diff-guide-chip"
            title={path}
            onClick={() => onSelectPath(path)}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}

export default function DiffReview({
  handle,
  title,
  selectedPr,
  onBack,
  onSelectPr,
}: Props) {
  const { state } = useTaskDiffReviewQueries(handle, selectedPr);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [noiseExpanded, setNoiseExpanded] = useState(false);
  const autoOpenedRef = useRef(false);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;
  const { swiping, style, commit } = useSwipePageTransition(rootRef, {
    onRight: () => onBackRef.current?.(),
    shouldIgnoreTarget: isDiffPanGestureTarget,
    capture: false,
  });

  useEffect(() => {
    setSelectedPath(null);
    setNoiseExpanded(false);
    autoOpenedRef.current = false;
  }, [handle, selectedPr]);

  const { signalFiles, noiseFiles } = useMemo(() => {
    if (state.status !== "ready") {
      return { signalFiles: [] as DiffFileView[], noiseFiles: [] as DiffFileView[] };
    }
    return partitionFilesByRole(state.diff.files, state.diff.judgment.reading_order);
  }, [state]);

  useEffect(() => {
    if (state.status !== "ready" || autoOpenedRef.current) return;
    autoOpenedRef.current = true;
    const fromGuide = state.diff.judgment.reading_order[0];
    if (fromGuide && state.diff.files.some((file) => file.path === fromGuide)) {
      setSelectedPath(fromGuide);
      return;
    }
    const topSignal = signalFiles[0];
    if (topSignal) setSelectedPath(topSignal.path);
  }, [state, signalFiles]);

  const activeFile = useMemo(() => {
    if (state.status !== "ready" || !selectedPath) return null;
    return state.diff.files.find((file) => file.path === selectedPath) ?? null;
  }, [state, selectedPath]);

  const heading = title || handle;
  const githubUrl =
    state.status === "ready" ? (state.diff.pr?.url ?? state.prs.find((p) => p.number === selectedPr)?.url) : null;
  const totals = state.status === "ready" ? state.diff.judgment.totals : null;

  return (
    <div
      ref={rootRef}
      className={`diff-review${swiping ? " is-diff-swiping" : ""}`}
      data-testid="diff-review"
      style={style}
    >
      <div className="detail-header" data-testid="diff-review-header">
        <button type="button" className="back" onClick={() => commit("right")}>
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
          {state.phase === "pull-requests"
            ? "Loading pull requests…"
            : "Loading diff…"}
        </p>
      ) : null}

      {state.status === "error" ? (
        <p className="diff-status diff-error" data-testid="diff-error">
          {state.message}
        </p>
      ) : null}

      {state.status === "ready" && totals ? (
        <>
          {state.prListError ? (
            <p className="diff-status diff-error" data-testid="diff-pr-list-error">
              Could not load pull requests — {state.prListError}
            </p>
          ) : null}
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

          {state.diff.fell_back_from_pr != null ? (
            <p className="diff-status" data-testid="diff-fallback-banner">
              PR #{state.diff.fell_back_from_pr} patch unavailable — showing local
              base…HEAD diff.
            </p>
          ) : null}

          <p className="diff-source" data-testid="diff-source">
            Source: {state.diff.source}
          </p>

          <p className="diff-orientation" data-testid="diff-orientation">
            {totals.files} files · {totals.signal} signal · {totals.noise} noise · +
            {totals.additions} −{totals.deletions}
          </p>
          <JudgmentFlags
            flags={state.diff.judgment.flags}
            onSelectPath={(path) => setSelectedPath(path)}
          />
          <GuideStrip
            paths={state.diff.judgment.reading_order}
            onSelectPath={(path) => setSelectedPath(path)}
          />

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
              {signalFiles.map((file) => (
                <FileRow
                  key={file.path}
                  file={file}
                  onSelect={() => setSelectedPath(file.path)}
                />
              ))}
              {noiseFiles.length > 0 ? (
                <li className="diff-noise-section">
                  <button
                    type="button"
                    className="diff-noise-toggle"
                    data-testid="diff-noise-toggle"
                    aria-expanded={noiseExpanded}
                    onClick={() => setNoiseExpanded((expanded) => !expanded)}
                  >
                    {noiseExpanded ? "Hide noise" : `${noiseFiles.length} noise`}
                  </button>
                  {noiseExpanded ? (
                    <ul className="diff-file-list diff-noise-list">
                      {noiseFiles.map((file) => (
                        <FileRow
                          key={file.path}
                          file={file}
                          onSelect={() => setSelectedPath(file.path)}
                        />
                      ))}
                    </ul>
                  ) : null}
                </li>
              ) : null}
            </ul>
          )}
        </>
      ) : null}
    </div>
  );
}
