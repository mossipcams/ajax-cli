// Render batching only: rAF-coalesce streamed assistant/thought updates to the
// latest full-content text per itemId. The host normalizes ACP text into full
// updates; this layer does not merge deltas. Boundary events still flush any
// pending lane before they reach the reducer.

import type { OutputContentBlock } from "@/shared/lib/liveSessionOutputContent";
import type { WebSessionServerEvent } from "./contracts";

type Dispatch = (event: WebSessionServerEvent) => void;

interface LaneState {
  role: "agent" | "thought";
  itemId: string;
  messageId?: string;
  text: string;
  contentBlocks?: OutputContentBlock[];
  /** Last payload dispatched for this lane; skips redundant reducer work. */
  sentKey?: string;
}

function laneKey(lane: LaneState): string {
  return JSON.stringify({
    text: lane.text,
    contentBlocks: lane.contentBlocks ?? [],
  });
}

function isStreamedLane(
  event: WebSessionServerEvent,
): event is WebSessionServerEvent & {
  type: "message";
  role: "agent" | "thought";
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
  private rafId: number | null = null;

  constructor(dispatch: Dispatch) {
    this.dispatch = dispatch;
  }

  push(event: WebSessionServerEvent): void {
    if (isStreamedLane(event)) {
      const lane = this.lanes.get(event.itemId);
      if (lane) {
        lane.role = event.role;
        lane.messageId = event.messageId ?? lane.messageId;
        lane.text = event.text;
        lane.contentBlocks = event.contentBlocks ?? lane.contentBlocks;
      } else {
        this.lanes.set(event.itemId, {
          role: event.role,
          itemId: event.itemId,
          messageId: event.messageId,
          text: event.text,
          ...(event.contentBlocks ? { contentBlocks: event.contentBlocks } : {}),
        });
      }
      this.scheduleFlush();
      return;
    }
    this.cancelScheduledFlush();
    this.flushAll();
    this.dispatch(event);
  }

  flushAll(): void {
    this.cancelScheduledFlush();
    this.flushPending();
    this.lanes.clear();
  }

  dispose(): void {
    this.cancelScheduledFlush();
    this.lanes.clear();
  }

  private scheduleFlush(): void {
    if (this.rafId !== null) return;
    this.rafId = requestAnimationFrame(() => {
      this.rafId = null;
      this.flushPending();
    });
  }

  private cancelScheduledFlush(): void {
    if (this.rafId === null) return;
    cancelAnimationFrame(this.rafId);
    this.rafId = null;
  }

  private flushPending(): void {
    for (const lane of this.lanes.values()) {
      const key = laneKey(lane);
      if ((!lane.text && !lane.contentBlocks?.length) || key === lane.sentKey) continue;
      lane.sentKey = key;
      this.dispatch({
        type: "message",
        role: lane.role,
        text: lane.text,
        itemId: lane.itemId,
        ...(lane.messageId ? { messageId: lane.messageId } : {}),
        ...(lane.contentBlocks?.length ? { contentBlocks: lane.contentBlocks } : {}),
      });
    }
  }
}
