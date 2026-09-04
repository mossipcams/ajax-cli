import { useEffect, useRef, type ComponentProps, type PointerEvent, type ReactNode } from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { toolCount, useChatSession, type ConversationItem } from "./session/public";
import { useHistoryWindow } from "./conversation/useHistoryWindow";
import { Conversation } from "./conversation/public";
import ChatLiveHead from "./status/ChatLiveHead";
import ChatModelPresentation from "./model/ChatModelPresentation";
import { useSessionModelNotice, useSessionModelSheet } from "./model/notice";
import { PermissionPanel } from "./permissions/public";
import { ElicitationPanel } from "./elicitation/public";
import { useSwipePageTransition } from "@/shared/hooks/useSwipePageTransition";
import { ChatScroller, ChatScrollThread, useChatScroller } from "./scrolling/public";
import { blurComposerOnPointerDown } from "./scrolling/composerBlur";
import {
  ChatComposer,
  ChatTranscriptTail,
  ComposerProvider,
} from "./composer/public";
import type { LiveSessionConfigOption } from "@/shared/lib/liveSessionConfig";
import type { ChatTaskAttention } from "./status/public";

/** Selectable transcript prose — page swipe must not steal iOS text selection. */
const CHAT_TRANSCRIPT_TEXT_SELECTOR =
  ".session-said, .session-reply, .session-note-text, .session-thread-empty";

function isChatTranscriptTextTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(target.closest(CHAT_TRANSCRIPT_TEXT_SELECTOR));
}

function hasActiveChatTranscriptTextSelection(): boolean {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return false;
  const anchor = selection.anchorNode;
  if (!anchor) return false;
  const element = anchor instanceof Element ? anchor : anchor.parentElement;
  if (!element) return false;
  return Boolean(element.closest(CHAT_TRANSCRIPT_TEXT_SELECTOR));
}

/** Ignore transcript touches only while native text highlighting is active. */
function shouldIgnoreChatTranscriptSwipeTarget(target: EventTarget | null): boolean {
  if (!isChatTranscriptTextTarget(target)) return false;
  return hasActiveChatTranscriptTextSelection();
}

interface Props {
  handle: string | null;
  detail: BrowserTaskDetail | null;
  detailStatus: "loading" | "ready" | "stale" | "error";
  onBack?: () => void;
  onOpenDiff?: () => void;
  onMutated?: () => void;
  headActions?: ReactNode;
  taskAttention?: ChatTaskAttention | null;
  workspaceHeader?: ReactNode;
  onSessionActivity?: (activity: {
    model: string;
    busy: boolean;
    sessionConfigOptions?: LiveSessionConfigOption[];
  }) => void;
}

function ChatSessionBody({
  handle,
  detail,
  workspaceHeader,
  headActions,
  taskAttention,
  view,
  connected,
  everOpened,
  activityAgeMs,
  sendPrompt,
  withdrawQueuedPrompt,
  sendCancel,
  sendClear,
  markStopped,
  applyConfigOption,
  respondPermission,
  respondElicitation,
  composerRef,
  notice,
  dismissNotice,
  modelSheetOpen,
  setModelSheetOpen,
  visibleItems,
}: {
  handle: string;
  detail: BrowserTaskDetail | null;
  workspaceHeader: ReactNode;
  headActions: ReactNode;
  taskAttention: ChatTaskAttention | null;
  view: ReturnType<typeof useChatSession>["view"];
  connected: boolean;
  everOpened: boolean;
  activityAgeMs: number;
  sendPrompt: ReturnType<typeof useChatSession>["sendPrompt"];
  withdrawQueuedPrompt: ReturnType<typeof useChatSession>["withdrawQueuedPrompt"];
  sendCancel: ReturnType<typeof useChatSession>["sendCancel"];
  sendClear: ReturnType<typeof useChatSession>["sendClear"];
  markStopped: ReturnType<typeof useChatSession>["markStopped"];
  applyConfigOption: ReturnType<typeof useChatSession>["applyConfigOption"];
  respondPermission: ReturnType<typeof useChatSession>["respondPermission"];
  respondElicitation: ReturnType<typeof useChatSession>["respondElicitation"];
  composerRef: React.RefObject<HTMLTextAreaElement | null>;
  notice: string | null;
  dismissNotice: () => void;
  modelSheetOpen: boolean;
  setModelSheetOpen: (open: boolean) => void;
  visibleItems: ConversationItem[];
}) {
  const { surfaceStyle, scrollToLatest } = useChatScroller();
  const { confirmedModel, configOptions, availableCommands, promptCapabilities } = view.model;

  return (
    <>
      <div
        className="session-chat-surface"
        data-testid="session-chat-surface"
        style={surfaceStyle}
      >
        {workspaceHeader}
        <ChatLiveHead
          view={view}
          taskAttention={taskAttention}
          activityAgeMs={activityAgeMs}
          connected={connected}
          permission={
            view.elicitation.decision ? (
              <ElicitationPanel
                decision={view.elicitation.decision}
                connected={connected}
                onAccept={(content) => respondElicitation("accept", content)}
                onDecline={() => respondElicitation("decline")}
                onCancel={() => respondElicitation("cancel")}
              />
            ) : view.permission.decision ? (
              <PermissionPanel
                decision={view.permission.decision}
                connected={connected}
                onApprove={() => respondPermission(true)}
                onReject={() => respondPermission(false)}
              />
            ) : null
          }
          actions={headActions}
          onStop={sendCancel}
        />

        <ComposerProvider
          handle={handle}
          connected={connected}
          busy={view.turn.busy}
          everOpened={everOpened}
          conversation={view.conversation}
          conversationRevision={view.revision}
          availableCommands={availableCommands}
          promptCapabilities={promptCapabilities}
          composerRef={composerRef}
          scrollToLatest={scrollToLatest}
          sendPrompt={sendPrompt}
          withdrawQueuedPrompt={withdrawQueuedPrompt}
          sendCancel={sendCancel}
          sendClear={sendClear}
          markStopped={markStopped}
        >
          <ChatScrollThread>
            <ChatTranscriptTail
              itemCount={view.conversation.length}
              conversation={<Conversation items={visibleItems} busy={view.turn.busy} />}
            />
          </ChatScrollThread>

          <ChatModelPresentation
            handle={handle}
            agent={detail?.agent}
            connected={connected}
            confirmedModel={confirmedModel}
            configOptions={configOptions}
            notice={notice}
            dismissNotice={dismissNotice}
            modelSheetOpen={modelSheetOpen}
            setModelSheetOpen={setModelSheetOpen}
            onApply={applyConfigOption}
            renderComposer={({ notice: noticeSlot, modelControl }) => (
              <ChatComposer notice={noticeSlot} modelControl={modelControl} />
            )}
          />
        </ComposerProvider>
      </div>
    </>
  );
}

function ChatSessionScroll({
  handle,
  detailStatus,
  view,
  composerRef,
  ...bodyProps
}: Omit<ComponentProps<typeof ChatSessionBody>, "visibleItems"> & {
  handle: string;
  detailStatus: Props["detailStatus"];
  view: ReturnType<typeof useChatSession>["view"];
  composerRef: React.RefObject<HTMLTextAreaElement | null>;
}) {
  const historyWindow = useHistoryWindow(view.conversation, `${handle}:${detailStatus}`);

  return (
    <ChatScroller
      revision={view.revision}
      sessionKey={`${handle}:${detailStatus}`}
      composerRef={composerRef}
      activityCount={toolCount(view.conversation)}
      historyScroll={{
        hasEarlier: historyWindow.hasEarlier,
        revealEarlier: historyWindow.revealEarlier,
        windowGeneration: historyWindow.windowGeneration,
      }}
    >
      <ChatSessionBody {...bodyProps} handle={handle} view={view} composerRef={composerRef} visibleItems={historyWindow.visibleItems} />
    </ChatScroller>
  );
}

export default function ChatSurface({
  handle,
  detail,
  detailStatus,
  onBack,
  onOpenDiff,
  onMutated,
  headActions = null,
  taskAttention = null,
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
    shouldIgnoreTarget: shouldIgnoreChatTranscriptSwipeTarget,
  });
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const { notice, showNotice, dismissNotice } = useSessionModelNotice();
  const { modelSheetOpen, setModelSheetOpen } = useSessionModelSheet();

  const {
    view,
    connected,
    everOpened,
    activityAgeMs,
    sessionModel,
    sessionConfigOptions,
    sendPrompt,
    withdrawQueuedPrompt,
    sendCancel,
    sendClear,
    markStopped,
    applyConfigOption,
    respondPermission,
    respondElicitation,
  } = useChatSession({ handle, detail, onMutated, onConfigError: showNotice });

  useEffect(() => {
    onSessionActivity?.({
      model: sessionModel,
      busy: view.turn.busy,
      sessionConfigOptions,
    });
  }, [sessionModel, sessionConfigOptions, view.turn.busy, onSessionActivity]);

  if (!handle) return null;

  return (
    <section
      ref={rootRef}
      className={`session-page session-chat${swiping ? " is-diff-swiping" : ""}`}
      data-testid="session-chat"
      data-handle={handle}
      style={style}
      onPointerDown={(event: PointerEvent<HTMLElement>) =>
        blurComposerOnPointerDown(event, composerRef)
      }
    >
      <ChatSessionScroll
        handle={handle}
        detailStatus={detailStatus}
        view={view}
        detail={detail}
        workspaceHeader={workspaceHeader}
        headActions={headActions}
        taskAttention={taskAttention}
        connected={connected}
        everOpened={everOpened}
        activityAgeMs={activityAgeMs}
        sendPrompt={sendPrompt}
        withdrawQueuedPrompt={withdrawQueuedPrompt}
        sendCancel={sendCancel}
        sendClear={sendClear}
        markStopped={markStopped}
        applyConfigOption={applyConfigOption}
        respondPermission={respondPermission}
        respondElicitation={respondElicitation}
        composerRef={composerRef}
        notice={notice}
        dismissNotice={dismissNotice}
        modelSheetOpen={modelSheetOpen}
        setModelSheetOpen={setModelSheetOpen}
      />
    </section>
  );
}
