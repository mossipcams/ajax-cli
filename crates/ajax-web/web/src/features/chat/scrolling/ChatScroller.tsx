import {
  createContext,
  useContext,
  useEffect,
  useRef,
  type CSSProperties,
  type ReactNode,
  type RefObject,
} from "react";
import { useChatScroll, type HistoryScrollControl } from "./useChatScroll";
import { useChatViewport } from "./useChatViewport";

interface ChatScrollerContextValue {
  surfaceStyle: CSSProperties | undefined;
  scrollToLatest: () => void;
  threadRef: RefObject<HTMLDivElement | null>;
  onThreadScroll: ReturnType<typeof useChatScroll>["onThreadScroll"];
  behind: boolean;
  hasEarlier: boolean;
  loadEarlier: () => number;
}

const ChatScrollerContext = createContext<ChatScrollerContextValue | null>(null);

export function useChatScroller(): ChatScrollerContextValue {
  const value = useContext(ChatScrollerContext);
  if (!value) {
    throw new Error("useChatScroller must be used within ChatScroller");
  }
  return value;
}

interface Props {
  revision: number;
  sessionKey: string;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  /** Opaque activity count for the jump label — scrolling does not inspect items. */
  activityCount?: number;
  historyScroll?: HistoryScrollControl;
  children: ReactNode;
}

/** Owns pinned/history scroll state, DOM observers, and keyboard viewport geometry. */
export default function ChatScroller({
  revision,
  sessionKey,
  composerRef,
  activityCount = 0,
  historyScroll,
  children,
}: Props) {
  const threadRef = useRef<HTMLDivElement | null>(null);
  const layoutTransitionRef = useRef(false);
  const seenActivityCountRef = useRef(0);
  const { pinnedRef, ignoreScrollIntentRef, behind, scrollToLatest, restoreLiveEdge, onThreadScroll, loadEarlier, hasEarlier } =
    useChatScroll({
      threadRef,
      revision,
      sessionKey,
      layoutTransitionRef,
      historyScroll,
    });
  const { surfaceStyle } = useChatViewport({
    threadRef,
    composerRef,
    pinnedRef,
    ignoreScrollIntentRef,
    layoutTransitionRef,
    onRestoreLiveEdge: restoreLiveEdge,
  });

  useEffect(() => {
    if (!behind) seenActivityCountRef.current = activityCount;
  }, [activityCount, behind]);

  const unseenSteps = behind
    ? Math.max(0, activityCount - seenActivityCountRef.current)
    : 0;

  return (
    <ChatScrollerContext.Provider
      value={{ surfaceStyle, scrollToLatest, threadRef, onThreadScroll, behind, hasEarlier, loadEarlier }}
    >
      {children}
      <ChatScrollJump unseenSteps={unseenSteps} />
    </ChatScrollerContext.Provider>
  );
}

export function ChatScrollThread({ children }: { children: ReactNode }) {
  const { threadRef, onThreadScroll, hasEarlier, loadEarlier } = useChatScroller();
  return (
    <div
      className="session-thread"
      ref={threadRef}
      data-testid="session-thread"
      onScroll={onThreadScroll}
    >
      <div className="session-thread-inner" data-testid="session-thread-inner">
        {hasEarlier ? (
          <button
            type="button"
            className="session-load-earlier"
            data-testid="session-load-earlier"
            onClick={() => loadEarlier()}
          >
            Load earlier
          </button>
        ) : null}
        {children}
      </div>
    </div>
  );
}

export function ChatScrollJump({ unseenSteps = 0 }: { unseenSteps?: number }) {
  const { behind, scrollToLatest } = useChatScroller();
  if (!behind) return null;
  return (
    <button
      type="button"
      className="session-jump"
      data-testid="session-jump"
      onClick={scrollToLatest}
    >
      Jump to latest
      {unseenSteps ? ` · ${unseenSteps} new ${unseenSteps === 1 ? "step" : "steps"}` : ""}
    </button>
  );
}
