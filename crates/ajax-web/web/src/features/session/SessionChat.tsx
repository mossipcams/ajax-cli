// DIRECTION CONTRACT — orchestration session (Operate)
//
// THESIS: this surface is an instrument with a live head, not a message list.
//   What the agent is doing right now — the tool it is in, the file it touches,
//   the decision it needs — holds one fixed panel that never scrolls away, and
//   everything finished falls into a transcript below that reads freely because
//   nothing pushes it. It refuses the messenger arrangement the category ships,
//   where streaming output, reasoning noise and the one approval you owe all
//   compete inside a single auto-scrolling column.
// OWN-WORLD: Ajax Cockpit, unchanged. Soft Charcoal paper steps, hairline
//   rules, Soft Steel Blue as the running signal, --tone for status, mono only
//   where the CLI speaks (tool kinds, paths, code), uppercase tracked micro
//   labels for chrome, pill actions >=44px, flat depth.
// STORY: the operator opens a session on a phone, sees one panel saying what
//   the agent is doing and whether it needs them, answers if asked, scrolls the
//   transcript for history, types to steer.
// FIRST VIEWPORT: header (back / title / status) -> live head (state label +
//   running tool + decision when one exists) -> settled transcript -> composer.
//   The primary action is whatever the head asks for; with nothing asked, the
//   composer is primary.
// FORM: candidate 6 of 7 ("instrument stack: live head over settled
//   transcript"), staging fused from the wound-medium challenger — live head
//   distinct from settled tape, honest position readout, jump-to-live. Seed key
//   361116ac, scope surface, mode operate.
// FINISH: unreviewed and undocumented is unfinished; this build ends with the
//   finish review, the verdict, and DESIGN.md.

import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useId,
  useReducer,
  useRef,
  useState,
  type FormEvent,
  type UIEvent,
} from "react";
import type { BrowserCockpitView, BrowserTaskDetail, WebAction } from "@/shared/lib/types";
import { statusMeta } from "@/shared/lib/state";
import { visibleTaskActions } from "@/features/task/taskActions";
import ActionBar from "@/features/task/ActionBar";
import TaskLoadError from "@/features/task/TaskLoadError";
import Skeleton from "@/shared/ui/Skeleton";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { Button } from "@/shared/ui/button";
import {
  connectWebSessionTransport,
  type WebSessionTransport,
} from "@/shared/lib/webSessionTransport";
import type { SessionStarterContext } from "./SessionStarter";
import {
  activeTool,
  initialSessionState,
  sessionReducer,
  toolCallCount,
  explainOpenFailure,
  OPEN_FAILURE,
  type ThreadEntry,
} from "./sessionThread";
import LiveHead, { headState, headTone } from "./LiveHead";
import Transcript from "./Transcript";

const TaskTerminal = lazy(() => import("@/features/task/TaskTerminal"));

/** Treat "within this many px of the bottom" as following the live edge. */
const PIN_THRESHOLD_PX = 48;
const RECONNECT_BASE_MS = 500;
const RECONNECT_MAX_MS = 8000;
/** Each attempt spawns an agent process host-side; never retry unbounded. */
const MAX_RECONNECT_ATTEMPTS = 5;

interface Props {
  handle: string | null;
  detail: BrowserTaskDetail | null;
  detailStatus: "loading" | "ready" | "stale" | "error";
  detailError?: string;
  starterContext?: SessionStarterContext | null;
  onBack?: () => void;
  onOpenDiff?: () => void;
  onCockpit?: (cockpit: BrowserCockpitView) => void;
  onResult?: (
    message: string,
    output: string | null | undefined,
    isError: boolean,
    options?: {
      onUndo?: () => void;
      onCommit?: () => void;
      pendingConfirm?: { action: WebAction; handle: string; interactionId: string };
    },
  ) => void;
  onMutated?: () => void;
  onDismiss?: () => void;
  onRetry?: () => void;
}

/** Grow the composer to its content. CSS `max-height` caps it, after which the
 * textarea scrolls internally — a one-row box that scrolls is unusable on a
 * phone, which is where this surface lives.
 *
 * Typing forward is the hot path and only ever needs to grow, so it skips the
 * reset entirely; the `height = "auto"` reset — which forces a synchronous
 * reflow on every keystroke — runs only when the text may have gotten shorter. */
function autoGrow(node: HTMLTextAreaElement, shrank: boolean) {
  if (shrank) node.style.height = "auto";
  else if (node.scrollHeight <= node.clientHeight) return;
  node.style.height = `${node.scrollHeight}px`;
}

export function formatSessionBrief(context: SessionStarterContext): string {
  const lines = [context.title.trim()];
  if (context.constraints.trim()) lines.push(`\nConstraints: ${context.constraints.trim()}`);
  if (context.expectedOutcome.trim()) lines.push(`\nDone when: ${context.expectedOutcome.trim()}`);
  return lines.join("\n");
}

export default function SessionChat({
  handle,
  detail,
  detailStatus,
  detailError,
  starterContext,
  onBack,
  onOpenDiff,
  onCockpit,
  onResult,
  onMutated,
  onDismiss,
  onRetry,
}: Props) {
  const composerId = useId();
  const threadRef = useRef<HTMLDivElement | null>(null);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const transportRef = useRef<WebSessionTransport | undefined>(undefined);
  // The starter brief seeds the ACP session exactly once. Holding it in a ref
  // keeps it out of the transport effect's deps — when it was a dependency, a
  // new object identity tore down the socket and killed the ACP child process
  // mid-turn.
  const starterRef = useRef(starterContext);
  // Read inside the transport effect without making it a dependency.
  const detailRef = useRef(detail);
  const seededRef = useRef(false);
  // What the operator had already seen when they last held the live edge.
  const seenRef = useRef<{ entries: ThreadEntry[]; tools: number }>({
    entries: [],
    tools: 0,
  });

  const [state, dispatch] = useReducer(sessionReducer, initialSessionState);
  const [draft, setDraft] = useState("");
  const [connected, setConnected] = useState(false);
  const [pinned, setPinned] = useState(true);
  const [behind, setBehind] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [terminalOpen, setTerminalOpen] = useState(false);

  starterRef.current = starterContext;
  detailRef.current = detail;

  const scrollToLive = useCallback(() => {
    const node = threadRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
    setPinned(true);
    setBehind(false);
  }, []);

  useEffect(() => {
    if (!handle) return;
    let disposed = false;
    let attempt = 0;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;

    const open = () => {
      const transport = connectWebSessionTransport(handle, {
        onReady: () => {
          attempt = 0;
          setConnected(true);
        },
        onEvent: (event) => {
          // The socket cannot report why an upgrade was refused, so swap its
          // blank failure for the reason the task detail already carries.
          if (event.type === "error" && event.message === OPEN_FAILURE) {
            dispatch({
              type: "event",
              event: { type: "error", message: explainOpenFailure(detailRef.current) },
            });
            return;
          }
          dispatch({ type: "event", event });
        },
        onClosed: () => {
          setConnected(false);
          if (disposed) return;
          // The socket owns a holder count on the ACP process; a dropped
          // connection must come back on its own or the session is stranded.
          // Bounded, though: every attempt spawns a fresh agent process on the
          // host, so a server-side failure must not retry forever.
          attempt += 1;
          if (attempt > MAX_RECONNECT_ATTEMPTS) {
            dispatch({
              type: "event",
              event: {
                type: "error",
                message: "Lost the session connection. Reopen the task to try again.",
              },
            });
            return;
          }
          retryTimer = setTimeout(
            open,
            Math.min(RECONNECT_BASE_MS * 2 ** (attempt - 1), RECONNECT_MAX_MS),
          );
        },
      });
      transportRef.current = transport;
    };

    open();
    return () => {
      disposed = true;
      if (retryTimer) clearTimeout(retryTimer);
      transportRef.current?.dispose();
      transportRef.current = undefined;
      setConnected(false);
    };
  }, [handle]);

  useEffect(() => {
    if (!handle) return;
    seededRef.current = false;
    dispatch({ type: "reset" });
  }, [handle]);

  // Seeded from an effect rather than from onReady: a transport that reports
  // ready synchronously does so before transportRef is assigned, which silently
  // dropped the brief.
  useEffect(() => {
    if (!connected || seededRef.current) return;
    const starter = starterRef.current;
    if (!starter) return;
    seededRef.current = true;
    const brief = formatSessionBrief(starter);
    transportRef.current?.sendPrompt(brief);
    dispatch({ type: "prompt", text: brief });
  }, [connected]);

  // Follow the live edge only while the operator is already at it. Yanking the
  // viewport back mid-read is what made a streaming turn impossible to follow.
  //
  // `behind` tracks output that arrived *since* the operator left the edge, so
  // it keys off the entries changing — not off the unpin itself, which would
  // announce "behind" on any upward scroll with nothing new to see.
  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    if (pinned) {
      node.scrollTop = node.scrollHeight;
      seenRef.current = { entries: state.entries, tools: toolCallCount(state) };
      return;
    }
    if (state.entries !== seenRef.current.entries) setBehind(true);
  }, [state.entries, pinned, state]);

  function onThreadScroll(event: UIEvent<HTMLDivElement>) {
    const node = event.currentTarget;
    const atLive = node.scrollHeight - node.scrollTop - node.clientHeight < PIN_THRESHOLD_PX;
    setPinned(atLive);
    if (atLive) setBehind(false);
  }

  function sendDraft() {
    const text = draft.trim();
    if (!text) return;
    // A closed socket drops the payload silently, so recording the turn as sent
    // would put a message in the transcript the agent never received.
    if (!connected) return;
    transportRef.current?.sendPrompt(text);
    dispatch({ type: "prompt", text });
    setDraft("");
    if (composerRef.current) {
      composerRef.current.style.height = "";
    }
    scrollToLive();
  }

  function submitComposer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    sendDraft();
  }

  function respondDecision(approved: boolean) {
    const decision = state.decision;
    if (!decision || !connected) return;
    transportRef.current?.respondPermission(decision.requestId, approved);
    dispatch({ type: "decided" });
  }

  if (!handle) return null;

  if (detailStatus === "loading") {
    return (
      <section className="session-page" data-testid="session-chat">
        <Skeleton testid="session-skeleton" rows={5} />
      </section>
    );
  }

  if (!detail) {
    return (
      <section className="session-page" data-testid="session-chat">
        <TaskLoadError message={detailError ?? "Task not found"} onRetry={() => onRetry?.()} />
      </section>
    );
  }

  const meta = statusMeta(detail.status);
  const actions = visibleTaskActions(detail.actions);
  // The head is a fast-tap surface and its action row sits at the same screen
  // position Approve occupies one state over, so it carries the next *safe*
  // action only. Destructive intents stay in Task details, deliberately slower.
  const safeActions = actions.filter((action) => !action.destructive);
  const state_ = headState(state.decision, state.busy, detail);
  const tone = headTone(state_, detail);
  const unseenTools = Math.max(0, toolCallCount(state) - seenRef.current.tools);
  const activity = detail.agent_activity ?? detail.live_status_summary ?? null;

  return (
    <section className="session-page session-chat" data-testid="session-chat" data-handle={handle}>
      <header className="session-header">
        <button type="button" className="session-header-back" onClick={onBack}>
          ← Back
        </button>
        <h1 className="session-title">{detail.title || detail.qualified_handle}</h1>
        {/* One state at a time: while the head is reporting a live state, a
            second lifecycle vocabulary beside it just contradicts it. */}
        {state_ === "idle" ? (
          <span className={`session-status-pill tone-${meta.tone}`}>{meta.label}</span>
        ) : null}
      </header>

      <LiveHead
        state={state_}
        tone={tone}
        detail={detail}
        decision={state.decision}
        tool={activeTool(state)}
        thought={state.thought}
        status={state.status}
        connected={connected}
        actions={
          safeActions.length ? (
            <div data-testid="session-head-actions">
              <ActionBar
                actions={safeActions}
                handle={detail.qualified_handle}
                onCockpit={onCockpit}
                onResult={onResult}
                onMutated={onMutated}
                onDismiss={onDismiss}
              />
            </div>
          ) : null
        }
        onApprove={() => respondDecision(true)}
        onReject={() => respondDecision(false)}
        onStop={() => transportRef.current?.sendCancel()}
        onOpenDetails={() => setDetailsOpen(true)}
      />

      <div
        className="session-thread"
        ref={threadRef}
        data-testid="session-thread"
        onScroll={onThreadScroll}
      >
        {state.entries.length === 0 ? (
          <p className="session-thread-empty" data-testid="session-thread-empty">
            Nothing yet. Send a message to steer the agent.
          </p>
        ) : (
          <Transcript entries={state.entries} />
        )}
      </div>

      {behind ? (
        <button
          type="button"
          className="session-jump"
          data-testid="session-jump"
          onClick={scrollToLive}
        >
          Jump to live
          {unseenTools ? ` · ${unseenTools} new ${unseenTools === 1 ? "step" : "steps"}` : ""}
        </button>
      ) : null}

      <form
        className="session-composer"
        data-testid="session-composer"
        aria-label="Session composer"
        onSubmit={submitComposer}
      >
        <textarea
          id={composerId}
          rows={1}
          enterKeyHint="send"
          placeholder={
            !connected ? "Reconnecting…" : state.busy ? "Steer the agent…" : "Message…"
          }
          aria-label="Message"
          ref={composerRef}
          value={draft}
          onChange={(e) => {
            const next = e.target.value;
            autoGrow(e.currentTarget, next.length < draft.length);
            setDraft(next);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              sendDraft();
            }
          }}
        />
        {/* While a decision is pending the head owns primacy, so Send drops its
            accent fill rather than competing with Approve for the eye. */}
        <Button
          type="submit"
          variant={state.decision ? "secondary" : "default"}
          disabled={!draft.trim() || !connected}
        >
          Send
        </Button>
      </form>

      {detailsOpen ? (
        <FullscreenLayer zIndex={50}>
          <Sheet open onOpenChange={(open) => !open && setDetailsOpen(false)}>
            <SheetContent asChild aria-describedby={undefined}>
              {/* This element IS the Radix content node, so a backdrop tap is
                  inside it and onPointerDownOutside never fires — the
                  target===currentTarget guard is ours, as in NewTaskSheet. */}
              <div
                className="session-sheet-scrim"
                onPointerDown={(event) => {
                  if (event.target === event.currentTarget) setDetailsOpen(false);
                }}
              >
                <div
                  className="session-details-sheet"
                  data-testid="session-task-panel"
                  role="dialog"
                  aria-modal="true"
                  aria-label="Task details"
                >
                  <div className="session-sheet-header">
                    <SheetTitle asChild>
                      <h2>Task details</h2>
                    </SheetTitle>
                    <Button type="button" variant="secondary" onClick={() => setDetailsOpen(false)}>
                      Close
                    </Button>
                  </div>

                  <div className="session-details-body">
                    <dl className="session-meta" data-testid="session-artifact-status">
                      {detail.status_explanation ? (
                        <>
                          <dt>Status</dt>
                          <dd>{detail.status_explanation}</dd>
                        </>
                      ) : null}
                      {activity ? (
                        <>
                          <dt>Activity</dt>
                          <dd data-testid="session-artifact-activity">{activity}</dd>
                        </>
                      ) : null}
                      <dt>Lifecycle</dt>
                      <dd>{detail.lifecycle}</dd>
                      <dt>Agent</dt>
                      <dd>{detail.agent}</dd>
                      <dt>Branch</dt>
                      <dd className="session-meta-mono">{detail.branch}</dd>
                    </dl>

                    {detail.runtime_observation_error ? (
                      <p className="session-sheet-warning">{detail.runtime_observation_error}</p>
                    ) : null}

                    {detail.annotations.length ? (
                      <ul className="session-annotations" data-testid="session-artifact-annotations">
                        {detail.annotations.map((line) => (
                          <li key={line}>{line}</li>
                        ))}
                      </ul>
                    ) : null}

                    <div className="session-sheet-actions" data-testid="session-quick-actions">
                      {actions.length ? (
                        <ActionBar
                          actions={actions}
                          handle={detail.qualified_handle}
                          onCockpit={onCockpit}
                          onResult={onResult}
                          onMutated={onMutated}
                          onDismiss={onDismiss}
                        />
                      ) : null}
                      <div className="session-sheet-tools">
                        {onOpenDiff ? (
                          <Button type="button" variant="secondary" onClick={onOpenDiff}>
                            Show diff
                          </Button>
                        ) : null}
                        <Button
                          type="button"
                          variant="secondary"
                          data-testid="session-terminal-toggle"
                          onClick={() => {
                            setDetailsOpen(false);
                            setTerminalOpen(true);
                          }}
                        >
                          Terminal
                        </Button>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </SheetContent>
          </Sheet>
        </FullscreenLayer>
      ) : null}

      {terminalOpen ? (
        <FullscreenLayer zIndex={50}>
          <Sheet open onOpenChange={(open) => !open && setTerminalOpen(false)}>
            <SheetContent asChild aria-describedby={undefined}>
              <div
                className="session-sheet-scrim"
                onPointerDown={(event) => {
                  if (event.target === event.currentTarget) setTerminalOpen(false);
                }}
              >
                <div
                  className="session-terminal-sheet"
                  data-testid="session-terminal-sheet"
                  role="dialog"
                  aria-modal="true"
                  aria-label="Task terminal"
                >
                  <div className="session-sheet-header">
                    <SheetTitle asChild>
                      <h2>Terminal</h2>
                    </SheetTitle>
                    <Button type="button" variant="secondary" onClick={() => setTerminalOpen(false)}>
                      Close
                    </Button>
                  </div>
                  <Suspense fallback={null}>
                    <TaskTerminal handle={detail.qualified_handle} />
                  </Suspense>
                </div>
              </div>
            </SheetContent>
          </Sheet>
        </FullscreenLayer>
      ) : null}
    </section>
  );
}
