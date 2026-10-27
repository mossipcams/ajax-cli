import { useState } from "react";

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
