// DIRECTION CONTRACT — orchestration session (Operate)
//
// THESIS: this surface is an instrument with a live head, not a message list.
//   What the agent is doing right now — the tool it is in, the file it touches,
//   the decision it needs — holds one fixed panel that never scrolls away.
//   Settled turns fall into a transcript as conversation plus one work summary,
//   not a tool trace. It refuses the messenger arrangement the category ships,
//   where streaming output, reasoning noise and the one approval you owe all
//   compete inside a single auto-scrolling column.
// OWN-WORLD: Ajax Cockpit, unchanged. Soft Charcoal paper steps, hairline
//   rules, Soft Steel Blue as the running signal, --tone for status, mono only
//   where the CLI speaks (tool kinds, paths, code), uppercase tracked micro
//   labels for chrome, pill actions >=44px, flat depth.
// STORY: the operator opens a session on a phone, sees one panel saying what
//   the agent is doing and whether it needs them, answers if asked, scrolls the
//   transcript for history, types to steer.
// FIRST VIEWPORT: live head (back / title / state + running tool / decision)
//   -> settled transcript (~80% of the band) with a full-width in-thread
//   composer (Enter sends; no Send chrome). The primary action is whatever the
//   head asks for; with nothing asked, the composer is primary.
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
import { visibleTaskActions } from "@/features/task/taskActions";
import ActionBar from "@/features/task/ActionBar";
import TaskLoadError from "@/features/task/TaskLoadError";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { Button } from "@/shared/ui/button";
import type { WebSessionTransport } from "@/shared/lib/webSessionTransport";
import type { SessionStarterContext } from "./SessionStarter";
import {
  activePlanStep,
  activeTool,
  initialSessionState,
  sessionReducer,
  toolCallCount,
  type ThreadEntry,
} from "./sessionThread";
import LiveHead, { headState, headTone } from "./LiveHead";
import Transcript from "./Transcript";
import SessionModelSelect from "./SessionModelSelect";
import { useSessionModelPreference } from "./sessionModel";
import { autoGrow } from "./sessionChatChrome";
import { formatSessionBrief, PIN_THRESHOLD_PX, sessionSeededStorageKey } from "./sessionChatSeed";
import { useSessionTransport } from "./useSessionTransport";
import { useSwipePageTransition } from "@/shared/hooks/useSwipePageTransition";

const TaskTerminal = lazy(() => import("@/features/task/TaskTerminal"));

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

export { formatSessionBrief, sessionSeededStorageKey } from "./sessionChatSeed";

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
  const rootRef = useRef<HTMLElement | null>(null);
  const onOpenDiffRef = useRef(onOpenDiff);
  onOpenDiffRef.current = onOpenDiff;
  const onBackRef = useRef(onBack);
  onBackRef.current = onBack;
  const { swiping, style } = useSwipePageTransition(rootRef, {
    onLeft: () => onOpenDiffRef.current?.(),
    onRight: () => onBackRef.current?.(),
  });
  const threadRef = useRef<HTMLDivElement | null>(null);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const transportRef = useRef<WebSessionTransport | undefined>(undefined);
  const connectedRef = useRef(false);
  const everOpenedRef = useRef(false);
  const draftRef = useRef("");
  const followUpQueuedRef = useRef(false);
  const lastQueuedTextRef = useRef<string | null>(null);
  // The starter brief seeds the ACP session exactly once. Holding it in a ref
  // keeps it out of the transport effect's deps — when it was a dependency, a
  // new object identity tore down the socket and killed the ACP child process
  // mid-turn.
  const starterRef = useRef(starterContext);
  // Read inside the transport effect without making it a dependency.
  const detailRef = useRef(detail);
  // What the operator had already seen when they last held the live edge.
  const seenRef = useRef<{ entries: ThreadEntry[]; tools: number }>({
    entries: [],
    tools: 0,
  });
  // Read inside the resize observer without resubscribing on every pin flip.
  const pinnedRef = useRef(true);
  const lastActivityAtRef = useRef(Date.now());

  const [state, dispatch] = useReducer(sessionReducer, initialSessionState);
  const [draft, setDraft] = useState("");
  const [connected, setConnected] = useState(false);
  const [everOpened, setEverOpened] = useState(false);
  const [sessionModel, setSessionModel] = useSessionModelPreference();
  const [pinned, setPinned] = useState(true);
  const [behind, setBehind] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [followUpQueued, setFollowUpQueued] = useState(false);
  const [activityAgeMs, setActivityAgeMs] = useState(0);

  const markActivity = useCallback(() => {
    lastActivityAtRef.current = Date.now();
    setActivityAgeMs(0);
  }, []);

  starterRef.current = starterContext;
  detailRef.current = detail;
  pinnedRef.current = pinned;
  connectedRef.current = connected;

  const scrollToLive = useCallback(() => {
    const node = threadRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
    setPinned(true);
    setBehind(false);
  }, []);

  useSessionTransport({
    handle,
    dispatch,
    detailRef,
    transportRef,
    connectedRef,
    everOpenedRef,
    onActivity: markActivity,
    setConnected,
    setEverOpened,
  });

  useEffect(() => {
    if (!state.busy) return;
    const timer = window.setInterval(
      () => setActivityAgeMs(Date.now() - lastActivityAtRef.current),
      30_000,
    );
    return () => window.clearInterval(timer);
  }, [state.busy]);

  // Seeded from an effect rather than from onReady: a transport that reports
  // ready synchronously does so before transportRef is assigned, which silently
  // dropped the brief. sessionStorage survives remounts so reconnect does not
  // send a second in-flight session/prompt.
  useEffect(() => {
    if (!connected || !handle) return;
    if (sessionStorage.getItem(sessionSeededStorageKey(handle))) return;
    const starter = starterRef.current;
    if (!starter) return;
    const transport = transportRef.current;
    if (!transport) return;
    const brief = formatSessionBrief(starter);
    try {
      markActivity();
      transport.sendPrompt(brief);
    } catch {
      return;
    }
    dispatch({ type: "prompt", text: brief });
    sessionStorage.setItem(sessionSeededStorageKey(handle), "1");
  }, [connected, handle, markActivity]);

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
    if (
      state.entries !== seenRef.current.entries ||
      toolCallCount(state) !== seenRef.current.tools
    ) {
      setBehind(true);
    }
  }, [state.entries, pinned, state]);

  // The effect above re-pins when *entries* change, which leaves every other
  // way the transcript loses height unhandled — the composer growing under a
  // multi-line draft, the head gaining a decision panel, the keyboard band
  // resizing. Each of those slid the live edge out from under a pinned reader
  // (a four-line draft moved it 62px), and the next message then snapped it
  // back. Observing the thread's own box catches all of them at once.
  //
  // Keyed on detailStatus, not []: the first render is the loading skeleton,
  // which has no thread to observe, and a mount-only effect would bail there
  // and never retry.
  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (pinnedRef.current) node.scrollTop = node.scrollHeight;
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [handle, detailStatus]);

  function onThreadScroll(event: UIEvent<HTMLDivElement>) {
    const node = event.currentTarget;
    const atLive = node.scrollHeight - node.scrollTop - node.clientHeight < PIN_THRESHOLD_PX;
    setPinned(atLive);
    if (atLive) setBehind(false);
  }

  useEffect(() => {
    if (!state.busy) {
      followUpQueuedRef.current = false;
      setFollowUpQueued(false);
      lastQueuedTextRef.current = null;
    }
  }, [state.busy]);

  function sendDraft() {
    if (!connected) return;
    const text = draftRef.current.trim();

    if (state.busy && followUpQueuedRef.current) {
      if (text && text !== lastQueuedTextRef.current) {
        transportRef.current?.sendPrompt(text);
        dispatch({ type: "prompt", text });
        lastQueuedTextRef.current = text;
      }
      transportRef.current?.sendCancel(true);
      followUpQueuedRef.current = false;
      setFollowUpQueued(false);
      draftRef.current = "";
      setDraft("");
      if (composerRef.current) composerRef.current.style.height = "";
      return;
    }

    if (!text) return;

    if (state.busy) {
      followUpQueuedRef.current = true;
      setFollowUpQueued(true);
      lastQueuedTextRef.current = text;
    }

    transportRef.current?.sendPrompt(text);
    if (!state.busy) markActivity();
    dispatch({ type: "prompt", text });
    draftRef.current = "";
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

  if (detailStatus === "error" || (detailStatus !== "loading" && !detail)) {
    return (
      <section className="session-page" data-testid="session-chat">
        <TaskLoadError message={detailError ?? "Task not found"} onRetry={() => onRetry?.()} />
      </section>
    );
  }

  const actions = detail ? visibleTaskActions(detail.actions) : [];
  const safeActions = actions.filter((action) => !action.destructive);
  const state_ = headState(state.decision, state.busy, detail);
  const tone = headTone(state_, detail);
  const unseenTools = Math.max(0, toolCallCount(state) - seenRef.current.tools);
  const activity = detail?.agent_activity ?? detail?.live_status_summary ?? null;
  const title = detail?.title || detail?.qualified_handle || handle;

  return (
    <section
      ref={rootRef}
      className={`session-page session-chat${swiping ? " is-diff-swiping" : ""}`}
      data-testid="session-chat"
      data-handle={handle}
      style={style}
    >
      <LiveHead
        title={title}
        state={state_}
        tone={tone}
        detail={detail}
        decision={state.decision}
        tool={activeTool(state)}
        planStep={activePlanStep(state.plan)}
        status={state.status}
        activityAgeMs={state.busy ? activityAgeMs : 0}
        connected={connected}
        actions={
          safeActions.length ? (
            <div data-testid="session-head-actions">
              <ActionBar
                actions={safeActions}
                handle={detail?.qualified_handle ?? handle}
                onCockpit={onCockpit}
                onResult={onResult}
                onMutated={onMutated}
                onDismiss={onDismiss}
              />
            </div>
          ) : null
        }
        onBack={onBack ?? (() => {})}
        onApprove={() => respondDecision(true)}
        onReject={() => respondDecision(false)}
        onStop={() => {
          transportRef.current?.sendCancel();
          followUpQueuedRef.current = false;
          setFollowUpQueued(false);
          lastQueuedTextRef.current = null;
        }}
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
            Message the agent to steer this task.
          </p>
        ) : (
          <Transcript entries={state.entries} busy={state.busy} />
        )}

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
              !connected
                ? everOpened
                  ? "Reconnecting…"
                  : "Starting…"
                : state.busy && followUpQueued
                  ? "Enter again to stop and send"
                  : state.busy
                    ? "Sends after this turn…"
                    : "Message…"
            }
            aria-label="Message"
            ref={composerRef}
            value={draft}
            onChange={(e) => {
              const next = e.target.value;
              draftRef.current = next;
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
        </form>
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
                      {detail?.status_explanation ? (
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
                      {detail ? (
                        <>
                          <dt>Lifecycle</dt>
                          <dd>{detail.lifecycle}</dd>
                          <dt>Agent</dt>
                          <dd>{detail.agent}</dd>
                          <dt>Branch</dt>
                          <dd className="session-meta-mono">{detail.branch}</dd>
                        </>
                      ) : null}
                    </dl>

                    <SessionModelSelect
                      id={`${composerId}-model`}
                      agent={detail?.agent}
                      value={sessionModel}
                      disabled={state.busy || !connected}
                      onChange={(id) => {
                        setSessionModel(id);
                        transportRef.current?.setModel(id);
                      }}
                    />

                    {detail?.runtime_observation_error ? (
                      <p className="session-sheet-warning">{detail.runtime_observation_error}</p>
                    ) : null}

                    {detail?.annotations.length ? (
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
                          handle={detail?.qualified_handle ?? handle}
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
                  <div className="session-terminal-body">
                    <Suspense fallback={null}>
                      <TaskTerminal handle={detail?.qualified_handle ?? handle} />
                    </Suspense>
                  </div>
                </div>
              </div>
            </SheetContent>
          </Sheet>
        </FullscreenLayer>
      ) : null}
    </section>
  );
}
