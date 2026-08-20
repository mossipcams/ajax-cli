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
//   surface for. Tool calls became first-class items that revise in place;
//   noise is controlled by collapse rather than by discarding. Permission
//   BUTTONS stay in the head: a control inside a scrolling column can leave the
//   screen mid-decision.
// REVISION (mobile chat): keeping every ACP item in the column made the surface
//   a protocol event log — the operator scrolled past reasoning, plans and tool
//   rows to find the sentence they asked for. The substance is still kept, one
//   tap away: the column now holds the conversation and one activity disclosure
//   per turn, which reads out the current operation while the turn runs and a
//   counted summary once it settles. Reveal is by paragraph, never by token.
//   A follow-up typed into a live turn is queued and editable rather than fired
//   into the host queue, so "stop and send" is one predictable Enter away and
//   the cancelled prompt never overlaps the one replacing it.
// OWN-WORLD: Ajax Cockpit, unchanged. Soft Charcoal paper steps, hairline
//   rules, Soft Steel Blue as the running signal, --tone for status, mono only
//   where the CLI speaks (tool kinds, paths, code), uppercase tracked micro
//   labels for chrome, pill actions >=44px, flat depth.
// STORY: the operator opens a session on a phone, sees one panel saying what
//   the agent is doing and whether it needs them, answers if asked, scrolls the
//   transcript for history, types to steer.
// FIRST VIEWPORT: shared task header, then live head (state + running tool /
//   decision / context pressure) -> conversation (~80% of the band) with a full-width
//   in-thread composer (Enter sends; no Send chrome). The primary action is
//   whatever the head asks for; with nothing asked, the composer is primary.
// FORM: candidate 6 of 7 ("instrument stack: live head over settled
//   transcript"), staging fused from the wound-medium challenger — live head
//   distinct from settled tape, honest position readout, jump-to-live. Seed key
//   361116ac, scope surface, mode operate.
// FINISH: unreviewed and undocumented is unfinished; this build ends with the
//   finish review, the verdict, and DESIGN.md.

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type FormEvent,
  type PointerEvent,
  type ReactNode,
  type UIEvent,
} from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import {
  activePlanStep,
  activeTool,
  latestPlan,
  latestThought,
  thoughtSnippet,
  toolCount,
  type ConversationItem,
} from "./sessionThread";
import LiveHead, { headState, headTone } from "./LiveHead";
import Transcript from "./Transcript";
import { autoGrow } from "./sessionChatChrome";
import { PIN_THRESHOLD_PX } from "./sessionChatSeed";
import { useTaskSession } from "./useTaskSession";
import ConfigPickers, {
  ConfigPickerNotice,
  hasConfigPickerControls,
  useConfigPickerNotice,
} from "./ConfigPickers";
import ModelSwitchSheet, { modelControlLabel } from "./ModelSwitchSheet";
import { useSwipePageTransition } from "@/shared/hooks/useSwipePageTransition";
import { useChatViewport } from "./viewport/useChatViewport";
import { useChatSpeech } from "./speech/useChatSpeech";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";

interface Props {
  handle: string | null;
  detail: BrowserTaskDetail | null;
  detailStatus: "loading" | "ready" | "stale" | "error";
  onBack?: () => void;
  onOpenDiff?: () => void;
  onMutated?: () => void;
  /** Task actions for the live head attention state — composed by Task Workspace. */
  headActions?: ReactNode;
  /** Shared task identity row owned by Task Workspace. */
  workspaceHeader?: ReactNode;
  /** Live session model, config options, and busy flag for workspace harness swap. */
  onSessionActivity?: (activity: {
    model: string;
    busy: boolean;
    sessionConfigOptions?: LiveSessionConfigOption[];
  }) => void;
}

export default function ChatSurface({
  handle,
  detail,
  detailStatus,
  onBack,
  onOpenDiff,
  onMutated,
  headActions = null,
  workspaceHeader = null,
  onSessionActivity,
}: Props) {
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
  const draftRef = useRef("");
  // What the operator had already seen when they last held the live edge.
  const seenRef = useRef<{ items: ConversationItem[] }>({ items: [] });
  // Read inside the resize observer without resubscribing on every pin flip.
  const pinnedRef = useRef(true);

  const [draft, setDraft] = useState("");
  const [pinned, setPinned] = useState(true);
  const [behind, setBehind] = useState(false);
  /** One editable follow-up held here until the active turn resolves. The host
   * still owns dispatch; holding it in the composer is what makes it editable
   * and what keeps a stop-and-send from racing the prompt it cancels. */
  const [queued, setQueued] = useState<string | null>(null);
  const [stopping, setStopping] = useState(false);
  const [modelSheetOpen, setModelSheetOpen] = useState(false);
  const { notice, showNotice, dismissNotice } = useConfigPickerNotice();

  const {
    state,
    connected,
    everOpened,
    activityAgeMs,
    sessionModel,
    sessionConfigOptions,
    sendPrompt,
    sendCancel,
    markStopped,
    applyConfigOption,
    respondPermission,
  } = useTaskSession({ handle, detail, onMutated, onConfigError: showNotice });

  useEffect(() => {
    onSessionActivity?.({ model: sessionModel, busy: state.busy, sessionConfigOptions });
  }, [sessionModel, sessionConfigOptions, state.busy, onSessionActivity]);

  const {
    speechModel,
    micAriaLabel,
    micArmed,
    toggleMic,
  } = useChatSpeech({
    handle: handle ?? "",
    draftRef,
    setDraft,
  });

  pinnedRef.current = pinned;

  const restoreLiveEdge = useCallback(() => {
    setPinned(true);
    setBehind(false);
  }, []);

  const { surfaceStyle } = useChatViewport({
    threadRef,
    composerRef,
    pinnedRef,
    onRestoreLiveEdge: restoreLiveEdge,
  });

  const scrollToLive = useCallback(() => {
    const node = threadRef.current;
    if (!node) return;
    node.scrollTop = node.scrollHeight;
    setPinned(true);
    setBehind(false);
  }, []);

  // Follow the live edge while the operator remains pinned. Scroll-up clears
  // `pinned` in onThreadScroll, so history readers are not yanked back.
  //
  // `behind` tracks output that arrived *since* the operator left the edge, so
  // it keys off the items changing — not off the unpin itself, which would
  // announce "behind" on any upward scroll with nothing new to see. A tool call
  // revising itself replaces its item, so this one identity check now covers
  // the tool activity that used to need a separate count.
  //
  // Layout effect, not effect: on open this runs before the browser paints, so
  // an existing conversation is already at its latest content rather than
  // painting at the top and scrolling down where the operator can see it.
  useLayoutEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    if (pinned) {
      node.scrollTop = node.scrollHeight;
      seenRef.current = { items: state.items };
      return;
    }
    if (state.items !== seenRef.current.items) setBehind(true);
  }, [state.items, pinned]);

  // The effect above re-pins when *entries* change. ResizeObserver catches
  // thread box resizes (composer growth, keyboard band). MutationObserver
  // catches scrollHeight growth inside the fixed-height scroller — streaming
  // lines appended after keyboard dismiss — which RO does not see.
  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => {
      if (!pinnedRef.current) return;
      node.scrollTop = node.scrollHeight;
    });
    observer.observe(node, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [handle, detailStatus]);

  // Observing the thread's border box catches layout-driven height changes the
  // items effect misses — composer growth under a multi-line draft, the head
  // gaining a decision panel, the keyboard band resizing.
  useEffect(() => {
    const node = threadRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (!pinnedRef.current) return;
      node.scrollTop = node.scrollHeight;
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

  function onPagePointerDown(event: PointerEvent<HTMLElement>) {
    const target = event.target as HTMLElement;
    if (target.closest("button, a, input, textarea, select, [role='button'], summary")) return;
    composerRef.current?.blur();
  }

  function clearDraft() {
    draftRef.current = "";
    setDraft("");
    if (composerRef.current) composerRef.current.style.height = "";
  }

  // Enter with a turn in flight queues one follow-up; Enter again stops the
  // turn and sends it. The send itself waits for the cancelled prompt to
  // resolve — see the flush effect below — so the two never run together.
  function sendDraft() {
    if (!connected) return;
    const text = draftRef.current.trim();

    if (queued !== null) {
      if (text) setQueued(text);
      clearDraft();
      if (state.busy && !stopping) {
        setStopping(true);
        sendCancel();
      }
      scrollToLive();
      return;
    }

    if (!text) return;
    if (state.busy) {
      setQueued(text);
      clearDraft();
      scrollToLive();
      return;
    }
    if (!sendPrompt(text)) return;
    clearDraft();
    scrollToLive();
  }

  function editQueued() {
    if (queued === null) return;
    draftRef.current = queued;
    setDraft(queued);
    setQueued(null);
    setStopping(false);
    composerRef.current?.focus();
  }

  // The turn is over — either normally or because Stop & send cancelled it —
  // so the follow-up becomes the next prompt. Nothing dispatches while
  // `state.busy`, which is the host's answer on whether a prompt is in flight.
  useEffect(() => {
    if (queued === null || state.busy || !connected) return;
    if (stopping) {
      markStopped();
      setStopping(false);
    }
    if (sendPrompt(queued)) setQueued(null);
  }, [queued, state.busy, connected, stopping, markStopped, sendPrompt]);

  function submitComposer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    sendDraft();
  }

  function respondDecision(approved: boolean) {
    respondPermission(approved);
  }

  if (!handle) return null;

  const state_ = headState(state.decision, state.busy, detail, state.status);
  const tone = headTone(state_, detail);
  const plan = latestPlan(state.items);
  const headTool = activeTool(state);
  const headPlanStep = activePlanStep(plan);
  const hasHeadWork = Boolean(headTool || headPlanStep);
  const headThought =
    state_ === "working" && !hasHeadWork
      ? (() => {
          const text = latestThought(state.items);
          return text ? thoughtSnippet(text) : null;
        })()
      : null;
  const unseenTools = Math.max(
    0,
    toolCount(state.items) - toolCount(seenRef.current.items),
  );
  // The action names what Enter does next, so the phone operator never has to
  // know the turn state to predict it.
  const submitLabel = queued !== null ? "Stop & send" : state.busy ? "Queue" : "Send";
  const modelPanelId = handle ? `session-model-${handle}` : "session-model";
  const showModelControl =
    Boolean(sessionConfigOptions?.length) &&
    hasConfigPickerControls(detail?.agent, sessionConfigOptions);
  const modelButtonLabel = modelControlLabel(sessionModel, sessionConfigOptions);

  return (
    <section
      ref={rootRef}
      className={`session-page session-chat${swiping ? " is-diff-swiping" : ""}`}
      data-testid="session-chat"
      data-handle={handle}
      style={style}
      onPointerDown={onPagePointerDown}
    >
      <div
        className="session-chat-surface"
        data-testid="session-chat-surface"
        style={surfaceStyle}
      >
      {workspaceHeader}
      <LiveHead
        state={state_}
        tone={tone}
        detail={detail}
        decision={state.decision}
        tool={headTool}
        planStep={headPlanStep}
        thoughtSnippet={headThought}
        usage={state.usage}
        turnUsage={state.turnUsage}
        activityAgeMs={state_ === "working" ? activityAgeMs : 0}
        connected={connected}
        actions={headActions}
        onApprove={() => respondDecision(true)}
        onReject={() => respondDecision(false)}
        onStop={sendCancel}
      />

      <div
        className="session-thread"
        ref={threadRef}
        data-testid="session-thread"
        onScroll={onThreadScroll}
      >
        {state.items.length === 0 && queued === null ? (
          <p className="session-thread-empty" data-testid="session-thread-empty">
            Message the agent to steer this task.
          </p>
        ) : (
          <Transcript items={state.items} busy={state.busy} />
        )}

        {queued !== null ? (
          <div className="session-queued" data-testid="session-queued">
            <p className="session-queued-label">{stopping ? "Stopping…" : "Queued"}</p>
            <article className="session-said is-queued">{queued}</article>
            {stopping ? null : (
              <p className="session-queued-hint">Press Enter again to stop and send now</p>
            )}
            <div className="session-queued-actions">
              <button type="button" onClick={editQueued}>
                Edit
              </button>
              <button
                type="button"
                onClick={() => {
                  setQueued(null);
                  setStopping(false);
                }}
              >
                Remove
              </button>
            </div>
          </div>
        ) : null}
      </div>

      <form
        className="session-composer"
        data-testid="session-composer"
        aria-label="Session composer"
        onSubmit={submitComposer}
      >
        {notice ? <ConfigPickerNotice message={notice} onDismiss={dismissNotice} /> : null}
        <div className="session-composer-row">
          <textarea
            rows={1}
            enterKeyHint="send"
            placeholder={
              !connected
                ? everOpened
                  ? "Reconnecting…"
                  : "Starting…"
                : queued !== null
                  ? "Enter stops this turn and sends…"
                  : state.busy
                    ? "Queues after this turn…"
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
            {showModelControl ? (
              <button
                type="button"
                className="session-composer-button session-composer-model"
                data-testid="session-model-open"
                aria-label="Choose model"
                aria-expanded={modelSheetOpen}
                aria-controls={modelPanelId}
                title={`Choose model — ${modelButtonLabel}`}
                disabled={!connected}
                onClick={() => setModelSheetOpen(true)}
              >
                <svg
                  className="session-composer-model-icon"
                  viewBox="0 0 24 24"
                  width="20"
                  height="20"
                  aria-hidden="true"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M12 3l1.5 4.5L18 9l-4.5 1.5L12 15l-1.5-4.5L6 9l4.5-1.5L12 3z" />
                  <path d="M19 14l1 3 3 1-3 1-1 3-1-3-3-1 3-1 1-3z" />
                </svg>
              </button>
            ) : null}
            <button
              type="button"
              className={`session-composer-button session-composer-mic${micArmed ? " is-armed" : ""}${speechModel.state === "connecting" ? " is-connecting" : ""}`}
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
              aria-label={submitLabel}
              disabled={!connected || (!draft.trim() && queued === null)}
            >
              {submitLabel}
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
          Jump to latest
          {unseenTools ? ` · ${unseenTools} new ${unseenTools === 1 ? "step" : "steps"}` : ""}
        </button>
      ) : null}

      {showModelControl ? (
        <ModelSwitchSheet
          open={modelSheetOpen}
          onOpenChange={setModelSheetOpen}
          panelId={modelPanelId}
          agent={detail?.agent}
          confirmedModel={sessionModel}
          options={sessionConfigOptions!}
          disabled={!connected}
          onApply={applyConfigOption}
        />
      ) : null}
    </section>
  );
}
