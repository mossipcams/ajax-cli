import posthog from "posthog-js";
import type { SupportedWebVitalsMetrics } from "posthog-js";
import {
  buildEventContext,
  isStandaloneDisplay,
  readAppVersion,
} from "./telemetryContext";
import { sanitizeTelemetryProps, type TelemetryProps } from "./telemetryFilter";
import {
  createMemoryTelemetryStore,
  openIndexedDbTelemetryStore,
  type TelemetryQueuedEvent,
  type TelemetryStore,
} from "./telemetryStore";
import { flushTelemetryQueue } from "./telemetryUpload";

/** Custom ignorelist replaces SDK defaults — include `.ph-no-autocapture` explicitly. */
export const POSTHOG_AUTOCAPTURE_IGNORELIST = [
  ".ph-no-autocapture",
  "[data-ph-no-autocapture]",
  "[data-sensitive]",
  '[data-testid="task-terminal-panel"]',
  '[data-testid="terminal-interaction-surface"]',
  ".terminal-host",
  ".terminal-interaction-wrap",
  ".xterm",
  "textarea.xterm-helper-textarea",
] as const;

const POSTHOG_DEFAULTS = "2026-05-30";
const DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com";
/** Ajax PostHog Cloud US project write key (browser). Override with `VITE_POSTHOG_KEY`. */
export const DEFAULT_POSTHOG_PROJECT_KEY =
  "phc_uQFMpY3C9L9Dj4wLqudjNyJVBwAdCisMyUkZ6EqhxWxB";
const WEB_VITALS_METRICS = [
  "LCP",
  "CLS",
  "FCP",
  "INP",
  "TTFB",
] as unknown as SupportedWebVitalsMetrics[];

function resolvePostHogKey(): string | undefined {
  const fromEnv = import.meta.env.VITE_POSTHOG_KEY?.trim();
  if (fromEnv === "0" || fromEnv === "off" || fromEnv === "disabled") {
    return undefined;
  }
  if (fromEnv) {
    return fromEnv;
  }
  return DEFAULT_POSTHOG_PROJECT_KEY;
}

let initialized = false;
let storeOverride: TelemetryStore | null = null;
let idbStorePromise: Promise<TelemetryStore | null> | null = null;
let flushPromise: Promise<void> | null = null;

export type { TelemetryProps };

export function isTelemetryInitialized(): boolean {
  return initialized;
}

/** Test seam: reset module init guard between unit tests. */
export function resetTelemetryForTests(): void {
  initialized = false;
  storeOverride = null;
  idbStorePromise = null;
  flushPromise = null;
  navigationStartedAt = null;
  navigationFromRoute = null;
  pwaLaunchCaptured = false;
}

/** Test seam: inject an in-memory telemetry store. */
export function setTelemetryStoreForTests(store: TelemetryStore | null): void {
  storeOverride = store;
  idbStorePromise = null;
}

export { isStandaloneDisplay, readAppVersion };

export function telemetryDistinctId(): string {
  return `ajax:${window.location.hostname}`;
}

function ensureStore(): Promise<TelemetryStore | null> {
  if (storeOverride) {
    return Promise.resolve(storeOverride);
  }
  if (typeof indexedDB === "undefined") {
    return Promise.resolve(null);
  }
  if (!idbStorePromise) {
    idbStorePromise = openIndexedDbTelemetryStore().catch((error) => {
      console.warn("[ajax] telemetry store open failed", error);
      return null;
    });
  }
  return idbStorePromise;
}

function captureQueuedEvent(
  event: string,
  properties: Record<string, string | number | boolean>,
): void {
  posthog.capture(event, properties);
}

async function runFlush(store: TelemetryStore): Promise<void> {
  let hasMore = true;
  while (hasMore) {
    hasMore = await flushTelemetryQueue(store, captureQueuedEvent);
  }
}

function scheduleFlush(): void {
  if (!initialized) {
    return;
  }
  void ensureStore()
    .then((store) => {
      if (!store) {
        return;
      }
      if (flushPromise) {
        return flushPromise.then(() => runFlush(store));
      }
      flushPromise = runFlush(store).finally(() => {
        flushPromise = null;
      });
      return flushPromise;
    })
    .catch((error) => {
      console.warn("[ajax] telemetry flush failed", error);
    });
}

export async function getTelemetryQueueStatus(): Promise<{
  pending: number;
  initialized: boolean;
}> {
  if (!initialized) {
    return { pending: 0, initialized: false };
  }
  try {
    const store = await ensureStore();
    if (!store) {
      return { pending: 0, initialized: true };
    }
    return { pending: await store.countPending(), initialized: true };
  } catch (error) {
    console.warn("[ajax] telemetry queue status failed", error);
    return { pending: 0, initialized: true };
  }
}

export function initTelemetry(): void {
  if (initialized) {
    return;
  }

  const key = resolvePostHogKey();
  if (!key) {
    return;
  }

  const host =
    import.meta.env.VITE_POSTHOG_HOST?.trim() || DEFAULT_POSTHOG_HOST;

  try {
    posthog.init(key, {
      api_host: host,
      defaults: POSTHOG_DEFAULTS,
      autocapture: {
        css_selector_ignorelist: [...POSTHOG_AUTOCAPTURE_IGNORELIST],
      },
      capture_performance: {
        web_vitals: true,
        web_vitals_allowed_metrics: [...WEB_VITALS_METRICS],
      },
      disable_session_recording: true,
      capture_exceptions: false,
    });

    const appVersion = readAppVersion();
    posthog.identify(telemetryDistinctId(), {
      host: window.location.hostname,
      origin: window.location.origin,
      ...(appVersion ? { app_version: appVersion } : {}),
    });

    initialized = true;
    scheduleFlush();
  } catch (error) {
    console.warn("[ajax] telemetry init failed", error);
  }
}

/** Capture a custom event when telemetry is initialized; no-op otherwise. */
export function track(event: string, properties?: TelemetryProps): void {
  if (!initialized) {
    return;
  }
  try {
    const context = buildEventContext();
    const sanitized = sanitizeTelemetryProps(properties ?? {});
    // Context wins — callers must not override event_id/sequence/standalone/etc.
    const merged = { ...sanitized, ...context };
    const record: TelemetryQueuedEvent = {
      event_id: context.event_id,
      event_name: event,
      properties: merged,
      created_at: Date.now(),
      attempts: 0,
      next_attempt_at: 0,
    };
    void ensureStore()
      .then(async (store) => {
        if (!store) {
          // IndexedDB unavailable — still deliver directly so metrics are not lost.
          captureQueuedEvent(event, merged);
          return;
        }
        await store.put(record);
        scheduleFlush();
      })
      .catch((error) => {
        console.warn("[ajax] telemetry enqueue failed", error);
        try {
          captureQueuedEvent(event, merged);
        } catch (captureError) {
          console.warn("[ajax] telemetry capture fallback failed", captureError);
        }
      });
  } catch (error) {
    console.warn("[ajax] telemetry capture failed", error);
  }
}

/** Alias for `track` — preserves prior call-site naming. */
export function captureEvent(event: string, properties?: TelemetryProps): void {
  track(event, properties);
}

type PendingInteraction = {
  control: string;
  startedAt: number;
  feedbackSent: boolean;
};

const pendingInteractions = new Map<string, PendingInteraction>();

/** Start a timed tap interaction; returns an id for feedback/complete ends. */
export function beginInteraction(control: string): string {
  const id = `${control}:${Math.random().toString(36).slice(2, 10)}`;
  pendingInteractions.set(id, {
    control,
    startedAt: performance.now(),
    feedbackSent: false,
  });
  return id;
}

export function endTapToFeedback(
  id: string,
  feedbackKind: string,
  extra?: TelemetryProps,
): void {
  const pending = pendingInteractions.get(id);
  if (!pending || pending.feedbackSent) {
    return;
  }
  pending.feedbackSent = true;
  track("ajax_tap_to_feedback", {
    control: pending.control,
    feedback_kind: feedbackKind,
    duration_ms: Math.round(performance.now() - pending.startedAt),
    ...extra,
  });
}

export function endTapToOperationComplete(
  id: string,
  props: { ok: boolean; op?: string; error_kind?: string } & TelemetryProps,
): void {
  const pending = pendingInteractions.get(id);
  if (!pending) {
    return;
  }
  pendingInteractions.delete(id);
  const { ok, op, error_kind, ...extra } = props;
  track("ajax_tap_to_operation_complete", {
    control: pending.control,
    op: op ?? pending.control,
    ok,
    ...(error_kind ? { error_kind } : {}),
    duration_ms: Math.round(performance.now() - pending.startedAt),
    ...extra,
  });
}

export function cancelInteraction(id: string): void {
  pendingInteractions.delete(id);
}

export type SwipeTelemetryProps = {
  direction: "left" | "right";
  duration_ms: number;
  distance_px: number;
  velocity_px_per_ms: number;
  completed: boolean;
  cancelled: boolean;
  settle_ms: number;
  from_route?: string;
  to_route?: string;
};

export function captureSwipe(props: SwipeTelemetryProps): void {
  track("ajax_swipe", props);
}

let navigationStartedAt: number | null = null;
let navigationFromRoute: string | null = null;
let pwaLaunchCaptured = false;

/** Mark navigation start for route-visible timing (before hash change). */
export function markNavigationStart(fromRoute?: string): void {
  navigationStartedAt = performance.now();
  navigationFromRoute = fromRoute ?? window.location.hash;
}

export function isNavigationPending(): boolean {
  return navigationStartedAt !== null;
}

export function captureRouteVisible(props?: {
  from_route?: string;
  to_route?: string;
  duration_ms?: number;
}): void {
  const duration_ms =
    props?.duration_ms ??
    (navigationStartedAt !== null
      ? Math.round(performance.now() - navigationStartedAt)
      : 0);
  track("ajax_route_visible", {
    duration_ms,
    ...(navigationFromRoute || props?.from_route
      ? { from_route: props?.from_route ?? navigationFromRoute }
      : {}),
    ...(props?.to_route ? { to_route: props.to_route } : {}),
  });
  navigationStartedAt = null;
  navigationFromRoute = null;
}

/** Once per cold boot — duration from navigation start to first shell visibility. */
export function capturePwaLaunch(duration_ms?: number): void {
  if (pwaLaunchCaptured) {
    return;
  }
  pwaLaunchCaptured = true;
  track("ajax_pwa_launch", {
    duration_ms: duration_ms ?? Math.round(performance.now()),
  });
}

export function capturePwaResume(props: { duration_ms: number }): void {
  track("ajax_pwa_resume", props);
}

export function captureTelemetryDiagnostic(): void {
  void getTelemetryQueueStatus()
    .then((status) => {
      const appVersion = readAppVersion();
      track("ajax_telemetry_diagnostic", {
        initialized: status.initialized,
        pending: status.pending,
        standalone: isStandaloneDisplay(),
        ...(appVersion ? { app_version: appVersion } : {}),
      });
    })
    .catch((error) => {
      console.warn("[ajax] telemetry diagnostic failed", error);
    });
}

/** Test helper: create a memory store with optional shared backing. */
export { createMemoryTelemetryStore };
