import { useState } from "react";

// The wire carries no per-message time, so the client stamps a message the first
// time it renders and remembers it per session. Messages that predate this
// feature show the time they were first loaded, not the time they were sent.
const ZONE = "America/Chicago";
const clock = new Intl.DateTimeFormat("en-US", {
  timeZone: ZONE,
  hour: "numeric",
  minute: "2-digit",
  timeZoneName: "short",
});
const day = new Intl.DateTimeFormat("en-US", { timeZone: ZONE, month: "short", day: "numeric" });

export function formatMessageTime(ms: number, now = Date.now()): string {
  const time = clock.format(ms);
  return day.format(ms) === day.format(now) ? time : `${day.format(ms)}, ${time}`;
}

const storageKey = (handle: string) => `ajax.web.chat.times.${encodeURIComponent(handle)}`;
const cache = new Map<string, Record<string, number>>();

function stamp(handle: string, itemId: string): number {
  let times = cache.get(handle);
  if (!times) {
    try {
      times = JSON.parse(localStorage.getItem(storageKey(handle)) ?? "{}") as Record<string, number>;
    } catch {
      times = {};
    }
    cache.set(handle, times);
  }
  if (times[itemId]) return times[itemId];
  times[itemId] = Date.now();
  try {
    localStorage.setItem(storageKey(handle), JSON.stringify(times));
  } catch {
    // ponytail: unbounded per-session map; prune if a transcript ever outgrows localStorage
  }
  return times[itemId];
}

export default function MessageTime({
  handle,
  itemId,
  role,
}: {
  handle: string;
  itemId: string;
  role: "user" | "agent";
}) {
  const [ms] = useState(() => stamp(handle, itemId));
  return (
    <time
      className="session-time"
      data-role={role}
      data-testid="session-message-time"
      dateTime={new Date(ms).toISOString()}
    >
      {formatMessageTime(ms)}
    </time>
  );
}
