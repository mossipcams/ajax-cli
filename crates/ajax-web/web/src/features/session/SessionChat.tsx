import {
  lazy,
  Suspense,
  useEffect,
  useId,
  useRef,
  useState,
  type Dispatch,
  type FormEvent,
  type SetStateAction,
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
  type WebSessionServerEvent,
  type WebSessionTransport,
} from "@/shared/lib/webSessionTransport";
import type { SessionStarterContext } from "./SessionStarter";

const TaskTerminal = lazy(() => import("@/features/task/TaskTerminal"));

export interface SessionThreadMessage {
  id: string;
  role: "user" | "agent" | "system";
  text: string;
}

type AttentionTarget = "status" | "activity" | "annotation";

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

function nextMessageId(): string {
  return `msg-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function formatStarterMessages(context: SessionStarterContext): SessionThreadMessage[] {
  const messages: SessionThreadMessage[] = [];
  if (context.constraints) {
    messages.push({ id: nextMessageId(), role: "user", text: `Constraints: ${context.constraints}` });
  }
  if (context.expectedOutcome) {
    messages.push({
      id: nextMessageId(),
      role: "user",
      text: `Expected outcome: ${context.expectedOutcome}`,
    });
  }
  return messages;
}

function starterPromptParts(context: SessionStarterContext): string[] {
  const parts: string[] = [];
  if (context.constraints.trim()) {
    parts.push(`Constraints: ${context.constraints.trim()}`);
  }
  if (context.expectedOutcome.trim()) {
    parts.push(`Expected outcome: ${context.expectedOutcome.trim()}`);
  }
  return parts;
}

function needsAttention(detail: BrowserTaskDetail): boolean {
  return detail.status === "waiting" || detail.status === "error";
}

function attentionLabel(detail: BrowserTaskDetail): string {
  if (detail.status_explanation?.trim()) return detail.status_explanation;
  if (detail.status === "waiting") return "Waiting for you";
  if (detail.status === "error") return "Needs attention";
  return "Needs attention";
}

function appendTransportEvent(
  setMessages: Dispatch<SetStateAction<SessionThreadMessage[]>>,
  setPermission: Dispatch<
    SetStateAction<{ requestId: string; title: string; detail: string } | null>
  >,
  event: WebSessionServerEvent,
) {
  if (event.type === "message" && event.text.trim()) {
    const role =
      event.role === "agent" ? "agent" : event.role === "user" ? "user" : "system";
    setMessages((prev) => [...prev, { id: nextMessageId(), role, text: event.text }]);
    return;
  }
  if (event.type === "artifact") {
    const title = event.title?.trim();
    const body = event.body?.trim();
    const text = [title ?? event.kind, body].filter(Boolean).join("\n");
    if (text) {
      setMessages((prev) => [
        ...prev,
        { id: nextMessageId(), role: "system", text: `Artifact (${event.kind}): ${text}` },
      ]);
    }
    return;
  }
  if (event.type === "permission_request") {
    setPermission({
      requestId: event.requestId,
      title: event.title?.trim() || "Permission required",
      detail: event.detail?.trim() || "",
    });
    setMessages((prev) => [
      ...prev,
      {
        id: nextMessageId(),
        role: "system",
        text: event.title?.trim() || "Agent needs permission",
      },
    ]);
    return;
  }
  if (event.type === "status" && event.state.trim()) {
    const detail = event.detail?.trim();
    setMessages((prev) => [
      ...prev,
      {
        id: nextMessageId(),
        role: "system",
        text: detail ? `Status: ${event.state} — ${detail}` : `Status: ${event.state}`,
      },
    ]);
  }
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
  const statusArtifactRef = useRef<HTMLElement | null>(null);
  const activityArtifactRef = useRef<HTMLElement | null>(null);
  const annotationArtifactRef = useRef<HTMLElement | null>(null);
  const threadSeededRef = useRef(false);
  const acpSeededRef = useRef(false);
  const transportRef = useRef<WebSessionTransport | undefined>(undefined);

  const [messages, setMessages] = useState<SessionThreadMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [composerError, setComposerError] = useState<string | null>(null);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [transportReady, setTransportReady] = useState(false);
  const [permission, setPermission] = useState<{
    requestId: string;
    title: string;
    detail: string;
  } | null>(null);

  useEffect(() => {
    if (!starterContext || threadSeededRef.current) return;
    const seeded = formatStarterMessages(starterContext);
    if (seeded.length) {
      setMessages((prev) => [...prev, ...seeded]);
    }
    threadSeededRef.current = true;
  }, [starterContext]);

  useEffect(() => {
    if (!handle) {
      transportRef.current?.dispose();
      transportRef.current = undefined;
      setTransportReady(false);
      return;
    }
    const transport = connectWebSessionTransport(handle, {
      onReady: () => {
        setTransportReady(true);
        queueMicrotask(() => {
          if (!starterContext || acpSeededRef.current) return;
          for (const part of starterPromptParts(starterContext)) {
            transportRef.current?.sendPrompt(part);
          }
          acpSeededRef.current = true;
        });
      },
      onEvent: (event) => {
        if (event.type === "error") {
          setComposerError(event.message);
          return;
        }
        appendTransportEvent(setMessages, setPermission, event);
      },
      onClosed: () => setTransportReady(false),
    });
    transportRef.current = transport;
    return () => {
      transport.dispose();
      if (transportRef.current === transport) {
        transportRef.current = undefined;
      }
    };
  }, [handle, starterContext]);

  const meta = detail ? statusMeta(detail.status) : null;
  const actions = detail ? visibleTaskActions(detail.actions) : [];
  const activityLine =
    detail && (detail.agent_activity ?? detail.live_status_summary) !== detail.status_explanation
      ? (detail.agent_activity ?? detail.live_status_summary)
      : null;
  const showAttention = detail ? needsAttention(detail) : false;

  function scrollToAttention(target: AttentionTarget) {
    const node =
      target === "status"
        ? statusArtifactRef.current
        : target === "activity"
          ? activityArtifactRef.current
          : annotationArtifactRef.current;
    node?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  function handleAttentionBannerClick() {
    if (!detail) return;
    if (detail.status_explanation?.trim()) {
      scrollToAttention("status");
      return;
    }
    if (activityLine) {
      scrollToAttention("activity");
      return;
    }
    scrollToAttention("annotation");
  }

  function submitComposer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const text = draft.trim();
    if (!text) return;
    setComposerError(null);
    transportRef.current?.sendPrompt(text);
    setMessages((prev) => [...prev, { id: nextMessageId(), role: "user", text }]);
    setDraft("");
    if (!transportReady) {
      setComposerError("Composer will deliver when the ACP session connects");
    }
  }

  if (!handle) {
    return null;
  }

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

  return (
    <section className="session-page session-chat" data-testid="session-chat" data-handle={handle}>
      <div className="session-header">
        <button type="button" className="back" onClick={onBack}>
          ← Back
        </button>
        <h1 className="session-title">{detail.title || detail.qualified_handle}</h1>
        {meta ? <span className={`interact-pill tone-${meta.tone}`}>{meta.label}</span> : null}
      </div>

      {showAttention ? (
        <button
          type="button"
          className="session-attention-banner"
          data-testid="session-attention-banner"
          onClick={handleAttentionBannerClick}
        >
          {attentionLabel(detail)}
        </button>
      ) : null}

      {permission ? (
        <div className="session-attention-banner" data-testid="session-permission-banner">
          <strong>{permission.title}</strong>
          {permission.detail ? <p>{permission.detail}</p> : null}
          <div className="session-diff-action">
            <Button
              type="button"
              variant="default"
              onClick={() => {
                transportRef.current?.respondPermission(permission.requestId, true);
                setPermission(null);
              }}
            >
              Approve
            </Button>
            <Button
              type="button"
              variant="secondary"
              onClick={() => {
                transportRef.current?.respondPermission(permission.requestId, false);
                setPermission(null);
              }}
            >
              Reject
            </Button>
          </div>
        </div>
      ) : null}

      <div className="session-thread" ref={threadRef} data-testid="session-thread">
        {messages.map((message) => (
          <article
            key={message.id}
            className={`session-message session-message-${message.role}`}
            data-testid={`session-message-${message.role}`}
          >
            <p>{message.text}</p>
          </article>
        ))}

        <article
          ref={statusArtifactRef}
          className="session-artifact session-artifact-status"
          data-testid="session-artifact-status"
        >
          <h2 className="session-artifact-label">Status</h2>
          {detail.runtime_observation_error ? (
            <p className="session-artifact-warning">{detail.runtime_observation_error}</p>
          ) : null}
          {detail.status_explanation ? <p>{detail.status_explanation}</p> : null}
          <p className="session-artifact-meta">
            {detail.lifecycle} · {detail.agent} · {detail.branch}
          </p>
        </article>

        {activityLine ? (
          <article
            ref={activityArtifactRef}
            className="session-artifact session-artifact-activity"
            data-testid="session-artifact-activity"
          >
            <h2 className="session-artifact-label">Activity</h2>
            <p>{activityLine}</p>
          </article>
        ) : null}

        {detail.annotations.length ? (
          <article
            ref={annotationArtifactRef}
            className="session-artifact session-artifact-annotations"
            data-testid="session-artifact-annotations"
          >
            <h2 className="session-artifact-label">Annotations</h2>
            <ul>
              {detail.annotations.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          </article>
        ) : null}

        <article className="session-artifact session-artifact-actions" data-testid="session-quick-actions">
          <h2 className="session-artifact-label">Quick actions</h2>
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
          <div className="session-diff-action">
            {onOpenDiff ? (
              <Button type="button" variant="secondary" onClick={onOpenDiff}>
                Show diff
              </Button>
            ) : null}
            <Button
              type="button"
              variant="secondary"
              onClick={() => {
                setDraft("Retry the last step");
              }}
            >
              Retry
            </Button>
            <Button
              type="button"
              variant="secondary"
              onClick={() => {
                setDraft("Try another approach");
              }}
            >
              Try another approach
            </Button>
          </div>
        </article>
      </div>

      <form
        className="session-composer"
        data-testid="session-composer"
        aria-label="Session composer"
        onSubmit={submitComposer}
      >
        <textarea
          id={composerId}
          rows={2}
          enterKeyHint="send"
          placeholder="Steer the agent…"
          aria-label="Message"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <div className="session-composer-actions">
          <Button
            type="button"
            variant="secondary"
            data-testid="session-terminal-toggle"
            onClick={() => setTerminalOpen(true)}
          >
            Terminal
          </Button>
          <Button type="submit" variant="default" disabled={!draft.trim()}>
            Send
          </Button>
        </div>
        {composerError ? <p className="session-composer-hint">{composerError}</p> : null}
      </form>

      {terminalOpen ? (
        <FullscreenLayer zIndex={50}>
          <Sheet
            open
            onOpenChange={(open) => {
              if (!open) setTerminalOpen(false);
            }}
          >
            <SheetContent aria-describedby={undefined}>
              <div
                className="session-terminal-sheet"
                data-testid="session-terminal-sheet"
                role="dialog"
                aria-modal="true"
                aria-label="Task terminal"
              >
                <div className="session-terminal-sheet-header">
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
            </SheetContent>
          </Sheet>
        </FullscreenLayer>
      ) : null}
    </section>
  );
}
