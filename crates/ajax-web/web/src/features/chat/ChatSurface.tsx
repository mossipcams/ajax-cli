import { useEffect, useRef, type PointerEvent, type ReactNode } from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { toolCount, useChatSession } from "./session/public";
import ChatLiveHead from "./status/ChatLiveHead";
import { Conversation } from "./conversation/public";
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
    sessionTitle?: string;
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
  sendCancel,
  markStopped,
  applyConfigOption,
  respondPermission,
  respondElicitation,
  composerRef,
  notice,
  dismissNotice,
  modelSheetOpen,
  setModelSheetOpen,
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
  sendCancel: ReturnType<typeof useChatSession>["sendCancel"];
  markStopped: ReturnType<typeof useChatSession>["markStopped"];
  applyConfigOption: ReturnType<typeof useChatSession>["applyConfigOption"];
  respondPermission: ReturnType<typeof useChatSession>["respondPermission"];
  respondElicitation: ReturnType<typeof useChatSession>["respondElicitation"];
  composerRef: React.RefObject<HTMLTextAreaElement | null>;
  notice: string | null;
  dismissNotice: () => void;
  modelSheetOpen: boolean;
  setModelSheetOpen: (open: boolean) => void;
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
          availableCommands={availableCommands}
          promptCapabilities={promptCapabilities}
          composerRef={composerRef}
          scrollToLatest={scrollToLatest}
          sendPrompt={sendPrompt}
          sendCancel={sendCancel}
          markStopped={markStopped}
        >
          <ChatScrollThread>
            <ChatTranscriptTail
              itemCount={view.conversation.length}
              conversation={<Conversation items={view.conversation} busy={view.turn.busy} />}
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
    shouldIgnoreTarget: isChatTranscriptTextTarget,
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
    sessionTitle,
    sendPrompt,
    sendCancel,
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
      sessionTitle,
    });
  }, [sessionModel, sessionConfigOptions, sessionTitle, view.turn.busy, onSessionActivity]);

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
      <ChatScroller
        revision={view.revision}
        sessionKey={`${handle}:${detailStatus}`}
        composerRef={composerRef}
        activityCount={toolCount(view.conversation)}
      >
        <ChatSessionBody
          handle={handle}
          detail={detail}
          workspaceHeader={workspaceHeader}
          headActions={headActions}
          taskAttention={taskAttention}
          view={view}
          connected={connected}
          everOpened={everOpened}
          activityAgeMs={activityAgeMs}
          sendPrompt={sendPrompt}
          sendCancel={sendCancel}
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
      </ChatScroller>
    </section>
  );
}
