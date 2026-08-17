// DIRECTION CONTRACT — orchestration session (Operate)
//
// THESIS: this surface is an instrument with a live head over a work record.
//   What the agent is doing right now — the tool it is in, the file it touches,
//   the decision it needs — holds one fixed panel that never scrolls away.
//   Below it, the turn is kept as the work it did: ACP separates message,
//   reasoning, tool call, tool content, plan and permission, and so does this
//   column. It refuses the messenger arrangement the category ships, where
//   streaming output, reasoning noise and the one approval you owe all compete
//   as undifferentiated prose in a single auto-scrolling column — but it
//   refuses equally the summary that replaces a turn's diff with "1 edit".
// REVISION (ACP-typed conversation): an earlier contract here settled a turn as
//   "conversation plus one work summary, not a tool trace". That threw away the
//   substance — the diff, the command output — that the operator opened the
//   surface for. Tool calls are now first-class items that revise in place;
//   noise is controlled by collapse (success collapsed, failure open, reasoning
//   one line) rather than by discarding. Permission BUTTONS stay in the head:
//   a control inside a scrolling column can leave the screen mid-decision.
// OWN-WORLD: Ajax Cockpit, unchanged. Soft Charcoal paper steps, hairline
//   rules, Soft Steel Blue as the running signal, --tone for status, mono only
//   where the CLI speaks (tool kinds, paths, code), uppercase tracked micro
//   labels for chrome, pill actions >=44px, flat depth.
// STORY: the operator opens a session on a phone, sees one panel saying what
//   the agent is doing and whether it needs them, answers if asked, scrolls the
//   transcript for history, types to steer.
// FIRST VIEWPORT: live head (back / title / state + running tool / decision /
//   context pressure) -> conversation (~80% of the band) with a full-width
//   in-thread composer (Enter sends; no Send chrome). The primary action is
//   whatever the head asks for; with nothing asked, the composer is primary.
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
  useRef,
  useState,
  type FormEvent,
  type UIEvent,
} from "react";
import type { Terminal } from "@xterm/xterm";
import type { BrowserCockpitView, BrowserTaskDetail, WebAction } from "@/shared/lib/types";
import type { TerminalConnection } from "@/shared/lib/terminalConnection";
import { visibleTaskActions } from "@/features/task/taskActions";
import ActionBar from "@/features/task/ActionBar";
import TaskLoadError from "@/features/task/TaskLoadError";
import FullscreenLayer from "@/shared/ui/FullscreenLayer";
import { Sheet, SheetContent, SheetTitle } from "@/shared/ui/sheet";
import { Button } from "@/shared/ui/button";
import {
  activePlanStep,
  activeTool,
  latestPlan,
  toolCount,
  type ConversationItem,
} from "./sessionThread";
import LiveHead, { headState, headTone } from "./LiveHead";
import Transcript from "./Transcript";
import SessionModelSelect from "./SessionModelSelect";
import { autoGrow } from "./sessionChatChrome";
import { PIN_THRESHOLD_PX } from "./sessionChatSeed";
import { useTaskSession } from "./useTaskSession";
import { useSwipePageTransition } from "@/shared/hooks/useSwipePageTransition";
import { useTaskTerminalSpeech } from "@/features/task/useTaskTerminalSpeech";

const TaskTerminal = lazy(() => import("@/features/task/TaskTerminal"));

interface Props {
  handle: string | null;
  detail: BrowserTaskDetail | null;
  detailStatus: "loading" | "ready" | "stale" | "error";
  detailError?: string;
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

export default function SessionChat({
  handle,
  detail,
  detailStatus,
  detailError,
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
  const speechTermRef = useRef<Terminal | undefined>(undefined);
  const speechConnectionRef = useRef<TerminalConnection | undefined>(undefined);
  const draftRef = useRef("");
  // What the operator had already seen when they last held the live edge.
  const seenRef = useRef<{ items: ConversationItem[] }>({ items: [] });
  // Read inside the resize observer without resubscribing on every pin flip.
  const pinnedRef = useRef(true);

  const [draft, setDraft] = useState("");
  const [pinned, setPinned] = useState(true);
  const [behind, setBehind] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [terminalOpen, setTerminalOpen] = useState(false);

  const {
    state,
    connected,
    everOpened,
    activityAgeMs,
    sessionModel,
    sendPrompt,
    sendCancel,
    setModel,
    respondPermission,
    onMutated: onSessionMutated,
  } = useTaskSession({ handle, detail, onMutated });

  const insertSpeechText = useCallback((text: string) => {
    const current = draftRef.current;
    const separator = current && !/\s$/.test(current) ? " " : "";
    const next = `${current}${separator}${text}`;
    draftRef.current = next;
    setDraft(next);
    return true;
  }, []);

  const {
    speechModel,
    micAriaLabel,
    micArmed,
    toggleMic,
  } = useTaskTerminalSpeech({
    handle: handle ?? "",
    termRef: speechTermRef,
    connectionRef: speechConnectionRef,
    pasteThroughTerm: insertSpeechText,
  });

  pinnedRef.current = pinned;

  const scrollToLive = useCallback(() => {
    const node = threadRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
    setPinned(true);
    setBehind(false);
  }, []);

  // Follow the live edge only while the operator is already at it. Yanking the
  // viewport back mid-read is what made a streaming turn impossible to follow.
  //
  // `behind` tracks output that arrived *since* the operator left the edge, so
  // it keys off the items changing — not off the unpin itself, which would
  // announce "behind" on any upward scroll with nothing new to see. A tool call
  // revising itself replaces its item, so this one identity check now covers
  // the tool activity that used to need a separate count.
  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    if (pinned) {
      node.scrollTop = node.scrollHeight;
      seenRef.current = { items: state.items };
      return;
    }
    if (state.items !== seenRef.current.items) setBehind(true);
  }, [state.items, pinned]);

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

  function sendDraft() {
    if (!connected) return;
    const text = draftRef.current.trim();
    if (!text) return;
    if (!sendPrompt(text)) return;
    draftRef.current = "";
    setDraft("");
    if (composerRef.current) composerRef.current.style.height = "";
    scrollToLive();
  }

  function submitComposer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    sendDraft();
  }

  function respondDecision(approved: boolean) {
    respondPermission(approved);
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
  const unseenTools = Math.max(
    0,
    toolCount(state.items) - toolCount(seenRef.current.items),
  );
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
        planStep={activePlanStep(latestPlan(state.items))}
        status={state.status}
        usage={state.usage}
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
                onMutated={onSessionMutated}
                onDismiss={onDismiss}
              />
            </div>
          ) : null
        }
        onBack={onBack ?? (() => {})}
        onApprove={() => respondDecision(true)}
        onReject={() => respondDecision(false)}
        onStop={sendCancel}
        onOpenDetails={() => setDetailsOpen(true)}
      />

      <div
        className="session-thread"
        ref={threadRef}
        data-testid="session-thread"
        onScroll={onThreadScroll}
      >
        {state.items.length === 0 ? (
          <p className="session-thread-empty" data-testid="session-thread-empty">
            Message the agent to steer this task.
          </p>
        ) : (
          <Transcript items={state.items} busy={state.busy} />
        )}

        <form
          className="session-composer"
          data-testid="session-composer"
          aria-label="Session composer"
          onSubmit={submitComposer}
        >
          <div className="session-composer-row">
            <textarea
              id={composerId}
              rows={1}
              enterKeyHint="send"
              placeholder={
                !connected
                  ? everOpened
                    ? "Reconnecting…"
                    : "Starting…"
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
            <div className="session-composer-actions">
              <button
                type="button"
                className={`session-composer-button session-composer-mic${micArmed ? " is-armed" : ""}`}
                aria-label={micArmed ? "Stop voice input" : micAriaLabel}
                title={micArmed ? "Stop voice input" : micAriaLabel}
                disabled={!connected || speechModel.state === "connecting" || speechModel.state === "finalizing"}
                onClick={toggleMic}
              >
                Mic
              </button>
              <button
                type="submit"
                className="session-composer-button session-composer-send"
                aria-label="Send"
                disabled={!connected || !draft.trim()}
              >
                Send
              </button>
            </div>
          </div>
          {speechModel.errorMessage || speechModel.state === "listening" ? (
            <p className="session-speech-status" role="status" aria-live="polite">
              {speechModel.errorMessage ?? "Listening…"}
            </p>
          ) : null}
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
                      onChange={(id) => setModel(id)}
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
                          onMutated={onSessionMutated}
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
