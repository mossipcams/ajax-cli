import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type Ref,
} from "react";
import type {
  AttentionBand,
  BrowserBackend,
  BrowserCockpitView,
  BrowserTaskCard,
  ConnectionState,
  RepoSummary,
  TaskStatus,
  WebAction,
} from "@/shared/lib/types";
import {
  filterByProject,
  formatDuration,
  isQuiet,
  relativeTime,
  sortCards,
  statusMeta,
} from "@/shared/lib/state";
import { visibleTaskActions } from "@/features/task/taskActions";
import ActionBar from "@/features/task/ActionBar";

/*
  THESIS: one armed channel owns the next safe intent in the thumb zone — this
  refuses both the card-per-task scroll and the dense CLI peg-rail drawer.
  OWN-WORLD: Ajax Cockpit unchanged — Soft Charcoal paper steps, hairline rules,
  mono handles as data, Soft Steel Blue primary, amber remediation.
  STORY: the operator lands with the host's lead task already armed, scans the
  fleet as thin traces above, and fires the intent with a thumb resting above
  the iOS bottom nav / home indicator.
  FIRST VIEWPORT: head (count · fleet words · repo select), band-tagged channel
  traces (glyph · handle · age), then a raised armed-channel card docked above
  the bottom nav — handle · age · Open, title, tone note, full-bleed primary +
  secondaries. Only filled pill on the page.
  FORM: grounded candidate 6 of 7 (channel focus), seed a355aa15; oscilloscope
  staging dressed in Ajax (one screen, many channels; switch to arm). iOS PWA
  dock: Comp A card language + bottom thumb placement.
  FINISH: unreviewed and undocumented is unfinished; this build ends with the
  finish review, the verdict, and DESIGN.md.
*/

interface Props {
  cockpit: BrowserCockpitView;
  connection: ConnectionState;
  selectedProject?: string | null;
  onSelectProject?: (project: string | null) => void;
  onOpenTask?: (handle: string) => void;
  onOpenSettings?: () => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onMutated?: () => void;
}

// Matt's operator hierarchy: what blocks me, what is moving, what is ready,
// what is history. Membership is always Rust's `card.attention`.
const BANDS: Array<[AttentionBand, string]> = [
  ["needs-you", "Needs you"],
  ["active", "Running"],
  ["review", "Ready"],
  ["idle", "Recent"],
];

const BAND_RANK = new Map<AttentionBand, number>(BANDS.map(([band], index) => [band, index]));
const BAND_LABEL = new Map<AttentionBand, string>(BANDS);

/** The CLI cockpit's status glyphs, so a roster row reads the way `ajax` reads
 * in a terminal. Tone carries the colour; the glyph carries the state. */
const GLYPH: Record<TaskStatus, string> = {
  running: "▸",
  waiting: "?",
  error: "!",
  idle: "·",
  unknown: "·",
};

/** Fallback dock height before the observer measures the real armed card.
 * Tall enough for title + note + primary + one full-width secondary. */
const RAIL_HEIGHT_FALLBACK = 240;

/** Every action the armed channel may run: Rust's list minus resume/open
 * (opening a task already resumes it) minus destructive (Drop on task detail). */
function safeActions(card: BrowserTaskCard): WebAction[] {
  return visibleTaskActions(card.actions).filter((action) => !action.destructive);
}

/** The one line that says what the task is doing. The server's explanation beats
 * the bare status word; a running task gone silent overrides both, because
 * "Running" is a lie once the pane stops moving. */
function statusLine(card: BrowserTaskCard, nowSecs: number): string {
  if (isQuiet(card, nowSecs)) {
    return `Stale ${formatDuration(nowSecs - card.last_activity_unix_secs)} — no output`;
  }
  return card.status_explanation || statusMeta(card.status).label;
}

function RosterRow({
  card,
  nowSecs,
  isSelected,
  onSelect,
}: {
  card: BrowserTaskCard;
  nowSecs: number;
  isSelected: boolean;
  onSelect: (handle: string) => void;
}) {
  const meta = statusMeta(card.status);
  const quiet = isQuiet(card, nowSecs);
  const band = BAND_LABEL.get(card.attention) ?? "";
  const node = useRef<HTMLButtonElement | null>(null);

  // The rail overlays the bottom of the scrollport, so a selected row can sit
  // under it — the tone wash tying row to rail is invisible exactly when it
  // matters. `scroll-margin-bottom` (set from the measured rail height) makes
  // "nearest" mean "clear of the rail".
  useEffect(() => {
    if (!isSelected) return;
    // jsdom has no scrollIntoView; the guard keeps unit tests off a shim.
    if (typeof node.current?.scrollIntoView !== "function") return;
    node.current.scrollIntoView({ block: "nearest" });
  }, [isSelected]);

  return (
    <li className="task-row-item">
      <button
        ref={node}
        type="button"
        className={`task-row tone-${quiet ? "waiting" : meta.tone}${isSelected ? " is-selected" : ""}${
          quiet ? " is-quiet" : ""
        }`}
        data-handle={card.qualified_handle}
        data-band={card.attention}
        data-testid={`task-row-${card.qualified_handle}`}
        aria-current={isSelected ? "true" : undefined}
        aria-label={`${card.qualified_handle}. ${band}. ${statusLine(card, nowSecs)}`}
        onClick={() => onSelect(card.qualified_handle)}
      >
        <span className="task-row-glyph" aria-hidden="true">
          {GLYPH[meta.tone]}
        </span>
        <span className="task-row-handle">{card.qualified_handle}</span>
        <span className="task-row-time">
          {relativeTime(card.last_activity_unix_secs, nowSecs)}
        </span>
      </button>
    </li>
  );
}

/** Armed channel: raised card docked in the iOS thumb zone above the bottom
 * nav. Traces above stay quiet; this card owns the only filled pill. */
function ArmedChannel({
  card,
  nowSecs,
  railRef,
  onOpenTask,
  onCockpit,
  onResult,
  onMutated,
}: {
  card: BrowserTaskCard;
  nowSecs: number;
  railRef: Ref<HTMLElement>;
  onOpenTask?: (handle: string) => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: Props["onResult"];
  onMutated?: () => void;
}) {
  const meta = statusMeta(card.status);
  const quiet = isQuiet(card, nowSecs);
  const actions = safeActions(card);

  return (
    <aside
      className={`task-rail tone-${quiet ? "waiting" : meta.tone}`}
      data-testid="task-rail"
      data-handle={card.qualified_handle}
      aria-label={`Armed channel ${card.qualified_handle}`}
      ref={railRef}
    >
      {/* Keyed so a new arm settles once and confirm/undo state never carries. */}
      <div className="rail-inner" key={card.qualified_handle}>
        <div className="rail-head">
          <span className="rail-handle">{card.qualified_handle}</span>
          <span className="rail-age">{relativeTime(card.last_activity_unix_secs, nowSecs)}</span>
          <button
            type="button"
            className="rail-open"
            onClick={() => onOpenTask?.(card.qualified_handle)}
          >
            Open<span aria-hidden="true"> ›</span>
          </button>
        </div>

        <p className="rail-title">{card.title || card.qualified_handle}</p>
        {/* Only the state line is live: re-reading handle/actions on every poll
            would drown the operator. */}
        <p className="rail-note" aria-live="polite">
          {statusLine(card, nowSecs)}
        </p>

        {actions.length > 0 ? (
          <div className="rail-actions" data-testid="rail-actions">
            <ActionBar
              layout="primary-key"
              actions={actions}
              handle={card.qualified_handle}
              onCockpit={onCockpit}
              onResult={onResult}
              onMutated={onMutated}
            />
          </div>
        ) : (
          <p className="rail-note rail-note--muted">No safe action from here — open the task.</p>
        )}
      </div>
    </aside>
  );
}

function repoChips(repo: RepoSummary): string[] {
  return [
    [repo.active_tasks ?? 0, "active"],
    [repo.attention_items ?? 0, "needs you"],
    [repo.reviewable_tasks ?? 0, "ready"],
    [repo.cleanable_tasks ?? 0, "cleanable"],
  ]
    .filter(([count]) => (count as number) > 0)
    .map(([count, label]) => `${count} ${label}`);
}

/** Reference, not tooling: repo inventory and backend authority hang at the tail,
 * closed, so the roster keeps the whole centre. */
function SystemFooter({
  backend,
  connection,
  repos,
  onOpenSettings,
}: {
  backend: BrowserBackend;
  connection: ConnectionState;
  repos: RepoSummary[];
  onOpenSettings?: () => void;
}) {
  return (
    <details className="fleet-footer" data-testid="fleet-footer">
      <summary className="fleet-summary">
        <span
          className={`system-dot${connection === "connected" ? " is-live" : ""}`}
          data-testid="fleet-link-dot"
          data-live={connection === "connected"}
          aria-hidden="true"
        />
        <span className="fleet-summary-label">System</span>
        <span className="fleet-summary-meta">
          {backend.authority}
          {backend.warning ? " · check" : ""}
        </span>
      </summary>

      <div className="fleet-body">
        {backend.warning ? <p className="system-warning">{backend.warning}</p> : null}

        <dl className="fleet-grid">
          <div>
            <dt>Authority</dt>
            <dd>{backend.authority}</dd>
          </div>
          <div>
            <dt>Control</dt>
            <dd>{backend.control_enabled ? "enabled" : "read-only"}</dd>
          </div>
        </dl>

        {repos.length > 0 ? (
          <ul className="repo-lines">
            {repos.map((repo) => {
              const chips = repoChips(repo);
              return (
                <li
                  className="repo-line"
                  key={repo.name}
                  data-repo={repo.name}
                  data-testid={`repo-line-${repo.name}`}
                >
                  <span className="repo-line-name">{repo.name}</span>
                  {repo.path ? (
                    <span className="repo-line-path">
                      <bdi>{repo.path}</bdi>
                    </span>
                  ) : null}
                  <span className="repo-line-counts">
                    {chips.length > 0 ? (
                      chips.map((chip) => (
                        <span className="repo-chip" key={chip}>
                          {chip}
                        </span>
                      ))
                    ) : (
                      <span className="repo-chip is-quiet">quiet</span>
                    )}
                  </span>
                </li>
              );
            })}
          </ul>
        ) : null}

        <button type="button" className="pill" onClick={() => onOpenSettings?.()}>
          Diagnostics
        </button>
      </div>
    </details>
  );
}

export default function Dashboard({
  cockpit,
  connection,
  selectedProject = null,
  onSelectProject,
  onOpenTask,
  onOpenSettings,
  onCockpit,
  onResult,
  onMutated,
}: Props) {
  const [nowSecs, setNowSecs] = useState(() => Math.floor(Date.now() / 1000));
  const [stableOrder, setStableOrder] = useState<string[]>([]);
  const [pinnedHandle, setPinnedHandle] = useState<string | null>(null);
  const [railHeight, setRailHeight] = useState(RAIL_HEIGHT_FALLBACK);
  const railNode = useRef<HTMLElement | null>(null);

  // Quiet detection turns on a 4-minute boundary, so the clock must tick faster
  // than the 60s row-time refresh to flip a running row to "quiet" on time.
  useEffect(() => {
    const timer = setInterval(() => setNowSecs(Math.floor(Date.now() / 1000)), 30_000);
    return () => clearInterval(timer);
  }, []);

  // The rail is fixed at one height, so the roster reserves exactly that much
  // room at its tail — measured, because the rail grows with wrapped actions.
  const measureRail = useCallback((node: HTMLElement | null) => {
    railNode.current = node;
    if (!node) return;
    setRailHeight(node.offsetHeight || RAIL_HEIGHT_FALLBACK);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      setRailHeight(node.offsetHeight || RAIL_HEIGHT_FALLBACK);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const repos = useMemo(() => cockpit.repos?.repos ?? [], [cockpit.repos?.repos]);

  const projects = useMemo(
    () =>
      [...new Set([...cockpit.cards.map((card) => card.repo), ...repos.map((r) => r.name)])].sort(),
    [cockpit.cards, repos],
  );

  const repoTaskCount = useMemo(() => {
    const counts = new Map<string, number>();
    for (const card of cockpit.cards) {
      counts.set(card.repo, (counts.get(card.repo) ?? 0) + 1);
    }
    return counts;
  }, [cockpit.cards]);

  // Rust ranks within a band (`sortCards`); the browser only keeps that order
  // stable across polls so rows don't move under a thumb mid-tap, then groups by
  // band. Array#sort is stable, so intra-band order survives.
  const rows = useMemo(
    () =>
      // sortCards already returns a fresh array, so this sorts a copy.
      sortCards(filterByProject(cockpit.cards, selectedProject), stableOrder).sort(
        (a, b) =>
          (BAND_RANK.get(a.attention) ?? BANDS.length) -
          (BAND_RANK.get(b.attention) ?? BANDS.length),
      ),
    [cockpit.cards, selectedProject, stableOrder],
  );

  useEffect(() => {
    const next = rows.map((card) => card.qualified_handle);
    setStableOrder((prev) =>
      next.length === prev.length && next.every((handle, i) => handle === prev[i]) ? prev : next,
    );
  }, [rows]);

  const bandCounts = useMemo(() => {
    const counts = new Map<AttentionBand, number>();
    for (const card of rows) {
      counts.set(card.attention, (counts.get(card.attention) ?? 0) + 1);
    }
    return counts;
  }, [rows]);

  // What the rail opens on: Rust sorts `inbox.items` by severity in
  // projection.rs, so the first entry still in view IS the next thing to do.
  // The browser reads that order only — `reason` is an evidence label it must
  // not translate. A tap pins a different row; a pin that leaves the view (drop,
  // repo filter, poll) falls back to the host's answer.
  const selected = useMemo(() => {
    const pinned = rows.find((card) => card.qualified_handle === pinnedHandle);
    if (pinned) return pinned;
    const handles = new Set(rows.map((card) => card.qualified_handle));
    const lead = cockpit.inbox?.items?.find((item) => handles.has(item.task_handle))?.task_handle;
    return rows.find((card) => card.qualified_handle === lead) ?? rows[0] ?? null;
  }, [cockpit.inbox?.items, pinnedHandle, rows]);

  const headline = BANDS.filter(([band]) => (bandCounts.get(band) ?? 0) > 0)
    .map(([band, label]) => `${bandCounts.get(band)} ${label.toLowerCase()}`)
    .join(" · ");

  const emptyMessage = selectedProject
    ? `No tasks in ${selectedProject} — start one with New.`
    : "All quiet — start a task with New.";

  return (
    <>
      <div className="roster-head">
        <span className="roster-count">
          <span className="roster-count-value">{rows.length}</span>
          <span className="roster-count-label">{rows.length === 1 ? "task" : "tasks"}</span>
        </span>
        <span className="roster-breakdown">{headline || "none live"}</span>
        {projects.length > 0 ? (
          <select
            className="repo-select"
            data-testid="repo-select"
            aria-label="Filter by repository"
            value={selectedProject ?? ""}
            onChange={(event) => onSelectProject?.(event.target.value || null)}
          >
            <option value="">All repos</option>
            {projects.map((project) => (
              <option key={project} value={project}>
                {project}
                {repoTaskCount.get(project) ? ` (${repoTaskCount.get(project)})` : ""}
              </option>
            ))}
          </select>
        ) : null}
      </div>

      {rows.length > 0 ? (
        <ul
          className="task-list"
          data-testid="roster"
          aria-label="Fleet roster"
          style={{ "--rail-h": `${railHeight}px` } as CSSProperties}
        >
          {rows.map((card, index) => {
            const previous = rows[index - 1];
            const opensBand = !previous || previous.attention !== card.attention;
            return (
              <Fragment key={card.qualified_handle}>
                {opensBand ? (
                  <li
                    className="task-band-rule"
                    data-testid={`band-rule-${card.attention}`}
                    aria-hidden="true"
                  >
                    <span className="task-band-label">{BAND_LABEL.get(card.attention)}</span>
                    <span className="task-band-count">{bandCounts.get(card.attention)}</span>
                  </li>
                ) : null}
                <RosterRow
                  card={card}
                  nowSecs={nowSecs}
                  isSelected={selected?.qualified_handle === card.qualified_handle}
                  onSelect={setPinnedHandle}
                />
              </Fragment>
            );
          })}
        </ul>
      ) : (
        <p className="empty">{emptyMessage}</p>
      )}

      <SystemFooter
        backend={cockpit.backend}
        connection={connection}
        repos={selectedProject ? repos.filter((repo) => repo.name === selectedProject) : repos}
        onOpenSettings={onOpenSettings}
      />

      {/* The rail is fixed, so the roster ends with exactly its height of room. */}
      {selected ? (
        <>
          <div
            className="rail-clearance"
            data-testid="rail-clearance"
            style={{ height: `${railHeight}px` }}
            aria-hidden="true"
          />
          <ArmedChannel
            card={selected}
            nowSecs={nowSecs}
            railRef={measureRail}
            onOpenTask={onOpenTask}
            onCockpit={onCockpit}
            onResult={onResult}
            onMutated={onMutated}
          />
        </>
      ) : null}
    </>
  );
}
