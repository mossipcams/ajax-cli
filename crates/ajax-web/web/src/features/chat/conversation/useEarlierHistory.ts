import { useCallback, useLayoutEffect, useRef, useState, type Dispatch } from "react";
import { projectWireEvent, type ChatSessionAction, type ChatSessionView } from "../session/public";
import { fetchEarlierTranscript } from "../session/transport/public";

interface Options {
  handle: string;
  serverHistory: ChatSessionView["serverHistory"];
  hasEarlierLocally: boolean;
  revealEarlier: () => number;
  dispatch: Dispatch<ChatSessionAction>;
  fetchImpl?: typeof fetch;
}

export function useEarlierHistory({
  handle,
  serverHistory,
  hasEarlierLocally,
  revealEarlier,
  dispatch,
  fetchImpl,
}: Options) {
  const [error, setError] = useState(false);
  const inFlightRef = useRef<AbortController | null>(null);
  const serverHistoryRef = useRef(serverHistory);
  serverHistoryRef.current = serverHistory;
  const hasEarlierOnServer = !!serverHistory && serverHistory.from > serverHistory.floor;

  useLayoutEffect(() => {
    setError(false);
    return () => {
      inFlightRef.current?.abort();
      inFlightRef.current = null;
    };
  }, [handle]);

  const loadEarlier = useCallback(() => {
    if (hasEarlierLocally) return revealEarlier();
    if (!serverHistory || !hasEarlierOnServer || inFlightRef.current) return 0;

    const requestedFrom = serverHistory.from;
    const requestedFloor = serverHistory.floor;
    const controller = new AbortController();
    inFlightRef.current = controller;
    setError(false);
    void (async () => {
      try {
        const page = await fetchEarlierTranscript(handle, requestedFrom, {
          fetchImpl,
          signal: controller.signal,
        });
        const current = serverHistoryRef.current;
        if (
          controller.signal.aborted || !current
          || current.from !== requestedFrom || current.floor !== requestedFloor
          || page.from >= current.from
        ) return;
        dispatch({
          type: "event",
          event: {
            type: "prepend",
            events: page.events.map(({ event }) => projectWireEvent(event))
              .filter((event) => event !== null),
            from: page.from,
          },
        });
      } catch {
        if (!controller.signal.aborted) setError(true);
      } finally {
        if (inFlightRef.current === controller) inFlightRef.current = null;
      }
    })();
    return 0;
  }, [dispatch, fetchImpl, handle, hasEarlierLocally, hasEarlierOnServer, revealEarlier, serverHistory]);

  return { hasEarlier: hasEarlierLocally || hasEarlierOnServer, loadEarlier, error };
}
