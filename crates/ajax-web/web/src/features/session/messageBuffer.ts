// Coalesce ACP message/thought chunks and render the assistant response as
// paragraphs, not as a token-by-token stream. ACP chunks are transport
// updates, not presentation units: rendering each one (or each phrase) makes a
// response read as word-by-word or sentence-by-sentence typing.
//
// Typewriter-free (issue #904): the buffer holds streamed text for the active
// turn and flushes only at a boundary — on `turn_end`, before any non-text
// event (so a tool call never appears above the prose that preceded it), and
// on `ready` (replayed history is already complete). The reducer stays pure
// and synchronous; only the dispatch cadence changes. While a turn streams,
// the head spinner and tool cards stay live; the assistant message renders as
// finished paragraphs when the turn — or a prose segment before a tool call —
// completes.
//
// `dispose` drops pending text without dispatching — the host replays its
// durable transcript on reconnect, so dropped-in-flight text is not lost truth.

import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";

type Dispatch = (event: WebSessionServerEvent) => void;

interface LaneState {
  role: "agent" | "thought";
  messageId?: string;
  text: string;
}

/** Only streamed assistant lanes are buffered. User echoes are single events,
 * not streamed responses, and pass through (after flushing any pending agent
 * text so the user turn lands in order). */
function isStreamedLane(
  event: WebSessionServerEvent,
): event is WebSessionServerEvent & { type: "message"; role: "agent" | "thought"; text: string } {
  return event.type === "message" && (event.role === "agent" || event.role === "thought");
}

function laneKey(role: "agent" | "thought", messageId?: string): string {
  return role === "thought" ? `thought:${messageId ?? ""}` : `prose:agent:${messageId ?? ""}`;
}

export class MessageBuffer {
  private lanes = new Map<string, LaneState>();
  private readonly dispatch: Dispatch;

  constructor(dispatch: Dispatch) {
    this.dispatch = dispatch;
  }

  /** Route a transport event: buffer streamed text, pass everything else
   * through after flushing pending text so ordering is preserved. */
  push(event: WebSessionServerEvent): void {
    if (isStreamedLane(event)) {
      this.accumulate(laneKey(event.role, event.messageId), event);
      return;
    }
    // A non-text event is a boundary: flush whatever prose accumulated so far
    // (turn_end ends the turn; a tool_call/plan/permission lands below the
    // words that came before it), then pass the event through.
    this.flushAll();
    this.dispatch(event);
  }

  /** Flush every pending lane synchronously. Used at boundaries (turn_end, a
   * non-text event via `push`) and on `ready` so replayed history renders. */
  flushAll(): void {
    for (const key of [...this.lanes.keys()]) this.flushLane(key);
  }

  /** Drop all pending text without dispatching. Use on teardown: a dispatch
   * after unmount is pointless, and the host replays the durable transcript on
   * reconnect. */
  dispose(): void {
    this.lanes.clear();
  }

  private accumulate(
    key: string,
    event: WebSessionServerEvent & { type: "message"; role: "agent" | "thought"; text: string },
  ): void {
    const lane = this.lanes.get(key) ?? { role: event.role, messageId: event.messageId, text: "" };
    // A harness may stream deltas or resend the cumulative snapshot so far.
    // Resolve like the reducer does so the buffer holds the true full text and
    // a flush dispatches one snapshot, not a delta the reducer would mis-merge.
    lane.text =
      event.text === lane.text
        ? lane.text
        : event.text.startsWith(lane.text) && event.text.length > lane.text.length
          ? event.text
          : lane.text + event.text;
    this.lanes.set(key, lane);
  }

  private flushLane(key: string): void {
    const lane = this.lanes.get(key);
    if (!lane) return;
    this.lanes.delete(key);
    if (!lane.text) return;
    this.dispatch({
      type: "message",
      role: lane.role,
      text: lane.text,
      ...(lane.messageId ? { messageId: lane.messageId } : {}),
    });
  }
}
