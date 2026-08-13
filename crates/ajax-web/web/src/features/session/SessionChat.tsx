import { lazy, Suspense, useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import type { BrowserCockpitView, BrowserTaskDetail, WebAction } from "@/shared/lib/types";
import { visibleTaskActions } from "@/features/task/taskActions";
import ActionBar from "@/features/task/ActionBar";
import TaskLoadError from "@/features/task/TaskLoadError";
import { Button } from "@/shared/ui/button";
import type { SessionStarterContext } from "./SessionStarter";
import { formatSessionBrief } from "./SessionStarter";
import LiveHead, { headState, headTone } from "./LiveHead";
import Transcript from "./Transcript";
import SessionModelSelect from "./SessionModelSelect";
import SessionComposer from "./SessionComposer";
import SessionSheet from "./SessionSheet";
import { useSessionModelPreference } from "./sessionModel";
import { activePlanStep, activeTool, projectSession } from "./projectSession";
import { useSessionConnection } from "./useSessionConnection";
import { useLiveThread } from "./useLiveThread";

// Live head + settled thread + composer on the keyboard band.
// Wire events fold through projectSession; this file only composes.
// Direction: DESIGN.md §5 scoped exception.

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

function seededKey(handle: string): string {
  return `ajax.web.session.seeded:${handle}`;
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
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const draftRef = useRef("");
  const queuedRef = useRef(false);
  const lastQueuedRef = useRef<string | null>(null);
  const starterRef = useRef(starterContext);
  starterRef.current = starterContext;

  const { events, connected, everOpened, offline, send } = useSessionConnection(handle, detail);
  const view = useMemo(() => projectSession(events), [events]);
  const live = useLiveThread(view.entries, view.tools.length, handle);

  const [draft, setDraft] = useState("");
  const [sessionModel, setSessionModel] = useSessionModelPreference();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [queuedFollowUp, setQueuedFollowUp] = useState(false);

  useEffect(() => {
    if (!connected || !handle) return;
    if (sessionStorage.getItem(seededKey(handle))) return;
    const starter = starterRef.current;
    if (!starter) return;
    sessionStorage.setItem(seededKey(handle), "1");
    send({ type: "prompt", text: formatSessionBrief(starter) });
  }, [connected, handle, send]);

  useEffect(() => {
    if (view.busy) return;
    queuedRef.current = false;
    setQueuedFollowUp(false);
    lastQueuedRef.current = null;
  }, [view.busy]);

  const sendDraft = useCallback(
    (fromComposer?: HTMLTextAreaElement | null) => {
      if (!connected) return;
      const field = fromComposer ?? composerRef.current;
      const raw = field?.value ?? draftRef.current;
      const text = raw.trim();

      if (view.busy && queuedRef.current) {
        if (text && text !== lastQueuedRef.current) {
          send({ type: "prompt", text });
          lastQueuedRef.current = text;
        }
        send({ type: "cancel", keepQueue: true });
        queuedRef.current = false;
        setQueuedFollowUp(false);
        draftRef.current = "";
        setDraft("");
        if (composerRef.current) composerRef.current.style.height = "";
        return;
      }

      if (!text) return;
      if (view.busy) {
        queuedRef.current = true;
        setQueuedFollowUp(true);
        lastQueuedRef.current = text;
      }
      if (!send({ type: "prompt", text })) return;
      draftRef.current = "";
      setDraft("");
      if (composerRef.current) composerRef.current.style.height = "";
      live.scrollToLive();
    },
    [connected, send, live, view.busy],
  );

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
  const state = headState(view.decision, view.busy, detail);
  const title = detail?.title || detail?.qualified_handle || handle;
  const activity = detail?.agent_activity ?? detail?.live_status_summary ?? null;

  return (
    <section className="session-page session-chat" data-testid="session-chat" data-handle={handle}>
      <LiveHead
        title={title}
        state={state}
        tone={headTone(state, detail)}
        detail={detail}
        decision={view.decision}
        tool={activeTool(view)}
        planStep={activePlanStep(view.plan)}
        status={view.status}
        connected={connected}
        offline={offline}
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
        onApprove={() => {
          if (view.decision && connected) {
            send({ type: "permission", requestId: view.decision.requestId, approved: true });
          }
        }}
        onReject={() => {
          if (view.decision && connected) {
            send({ type: "permission", requestId: view.decision.requestId, approved: false });
          }
        }}
        onStop={() => {
          send({ type: "cancel" });
          queuedRef.current = false;
          setQueuedFollowUp(false);
          lastQueuedRef.current = null;
        }}
        onOpenDetails={() => setDetailsOpen(true)}
      />

      <div
        className="session-thread"
        ref={live.threadRef}
        data-testid="session-thread"
        onScroll={live.onScroll}
      >
        {view.entries.length === 0 ? (
          <p className="session-thread-empty" data-testid="session-thread-empty">
            Message the agent to steer this task.
          </p>
        ) : (
          <Transcript entries={view.entries} busy={view.busy} />
        )}
        <SessionComposer
          composerId={composerId}
          composerRef={composerRef}
          connected={connected}
          everOpened={everOpened}
          busy={view.busy}
          queuedFollowUp={queuedFollowUp}
          draft={draft}
          onDraft={(next) => {
            draftRef.current = next;
            setDraft(next);
          }}
          onSubmit={sendDraft}
        />
      </div>

      {live.behind ? (
        <button type="button" className="session-jump" data-testid="session-jump" onClick={live.scrollToLive}>
          Jump to live
          {live.unseenTools
            ? ` · ${live.unseenTools} new ${live.unseenTools === 1 ? "step" : "steps"}`
            : ""}
        </button>
      ) : null}

      {detailsOpen ? (
        <SessionSheet
          testId="session-task-panel"
          label="Task details"
          title="Task details"
          className="session-details-sheet"
          onClose={() => setDetailsOpen(false)}
        >
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
              value={sessionModel}
              disabled={view.busy || !connected}
              onChange={(id) => {
                setSessionModel(id);
                send({ type: "set_model", model: id });
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
        </SessionSheet>
      ) : null}

      {terminalOpen ? (
        <SessionSheet
          testId="session-terminal-sheet"
          label="Task terminal"
          title="Terminal"
          className="session-terminal-sheet"
          onClose={() => setTerminalOpen(false)}
        >
          <div className="session-terminal-body">
            <Suspense fallback={null}>
              <TaskTerminal handle={detail?.qualified_handle ?? handle} />
            </Suspense>
          </div>
        </SessionSheet>
      ) : null}
    </section>
  );
}
