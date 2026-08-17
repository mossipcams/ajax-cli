// Render batching only: hold streamed assistant/thought updates until a boundary
// (turn_end, tool call, plan, permission, ready). The host normalizes ACP text
// into full-content item updates; this layer does not merge deltas.

import type { WebSessionServerEvent } from "@/shared/lib/webSessionTransport";

type Dispatch = (event: WebSessionServerEvent) => void;

interface LaneState {
  role: "agent" | "thought";
  itemId: string;
  messageId?: string;
  text: string;
}

function isStreamedLane(
  event: WebSessionServerEvent,
): event is WebSessionServerEvent & {
  type: "message";
  role: "agent" | "thought";
  text: string;
  itemId: string;
} {
  return (
    event.type === "message" &&
    (event.role === "agent" || event.role === "thought") &&
    typeof event.itemId === "string" &&
    !!event.itemId
  );
}

export class MessageBuffer {
  private lanes = new Map<string, LaneState>();
  private readonly dispatch: Dispatch;

  constructor(dispatch: Dispatch) {
    this.dispatch = dispatch;
  }

  push(event: WebSessionServerEvent): void {
    if (isStreamedLane(event)) {
      this.lanes.set(event.itemId, {
        role: event.role,
        itemId: event.itemId,
        messageId: event.messageId,
        text: event.text,
      });
      return;
    }
    this.flushAll();
    this.dispatch(event);
  }

  flushAll(): void {
    for (const lane of this.lanes.values()) {
      if (!lane.text) continue;
      this.dispatch({
        type: "message",
        role: lane.role,
        text: lane.text,
        itemId: lane.itemId,
        ...(lane.messageId ? { messageId: lane.messageId } : {}),
      });
    }
    this.lanes.clear();
  }

  dispose(): void {
    this.lanes.clear();
  }
}
