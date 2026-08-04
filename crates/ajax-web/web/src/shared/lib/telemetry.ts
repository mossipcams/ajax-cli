import posthog from "posthog-js";
import type { SupportedWebVitalsMetrics } from "posthog-js";
import { parseRoute } from "@/shared/lib/routes";
import {
  buildEventContext,
  generateId,
  getInstallId,
  isStandaloneDisplay,
  readAppVersion,
  readHost,
} from "./telemetryContext";
import { sanitizeTelemetryProps, type TelemetryProps } from "./telemetryFilter";

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

export type { TelemetryProps };

export function isTelemetryInitialized(): boolean {
  return initialized;
}

/** Test seam: reset module init guard between unit tests. */
export function resetTelemetryForTests(): void {
  initialized = false;
  navigationStartedAt = null;
  navigationFromRoute = null;
  navigationTrigger = null;
  pwaLaunchCaptured = false;
}

export { isStandaloneDisplay, readAppVersion };

export function telemetryDistinctId(): string {
  return `ajax:${window.location.hostname}`;
}

export function getTelemetryStatus(): {
  initialized: boolean;
  standalone: boolean;
} {
  return {
    initialized,
    standalone: isStandaloneDisplay(),
  };
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

    posthog.register({
      standalone: isStandaloneDisplay(),
      install_id: getInstallId(),
      host: readHost(),
      ...(appVersion ? { app_version: appVersion } : {}),
    });

    initialized = true;
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
    // Event props may override observational context (e.g. destination route_kind);
    // identity and sequence fields always win.
    posthog.capture(event, {
      ...context,
      ...sanitized,
      event_id: context.event_id,
      session_id: context.session_id,
      install_id: context.install_id,
      sequence: context.sequence,
      standalone: context.standalone,
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
  feedbackAt?: number;
  feedbackKind?: string;
};

type TapOutcome = "success" | "failed" | "cancelled";

function resolveTapOutcome(ok: boolean, error_kind?: string): TapOutcome {
  if (ok) {
    return "success";
  }
  if (
    error_kind === "confirm_timeout" ||
    error_kind === "undo" ||
    error_kind === "unmount"
  ) {
    return "cancelled";
  }
  return "failed";
}

const pendingInteractions = new Map<string, PendingInteraction>();

/** Start a timed tap interaction; returns an id for feedback/complete ends. */
export function beginInteraction(control: string): string {
  const id = `${control}:${generateId()}`;
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
  pending.feedbackAt = performance.now();
  pending.feedbackKind = feedbackKind;
  track("ajax_tap_to_feedback", {
    interaction_id: id,
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
    interaction_id: id,
    control: pending.control,
    op: op ?? pending.control,
    ok,
    outcome: resolveTapOutcome(ok, error_kind),
    ...(error_kind ? { error_kind } : {}),
    duration_ms: Math.round(performance.now() - pending.startedAt),
    ...(pending.feedbackAt !== undefined
      ? { feedback_ms: Math.round(pending.feedbackAt - pending.startedAt) }
      : {}),
    ...(pending.feedbackKind !== undefined
      ? { feedback_kind: pending.feedbackKind }
      : {}),
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
  page_width_px: number;
  velocity_px_per_ms: number;
  completed: boolean;
  cancelled: boolean;
  settle_ms: number;
  from_route?: string;
  to_route?: string;
};

export function captureSwipe(props: SwipeTelemetryProps): void {
  const page_width_px = props.page_width_px;
  const progress =
    page_width_px <= 0
      ? 0
      : Math.min(1, props.distance_px / page_width_px);
  const progressRounded = Math.round(progress * 1000) / 1000;
  const outcome = props.completed ? "completed" : "cancelled";
  track("ajax_swipe", {
    ...props,
    progress: progressRounded,
    outcome,
  });
}

let navigationStartedAt: number | null = null;
let navigationFromRoute: string | null = null;
let navigationTrigger: string | null = null;
let pwaLaunchCaptured = false;

/** Mark navigation start for route-visible timing (before hash change). */
export function markNavigationStart(fromRoute?: string, trigger?: string): void {
  navigationStartedAt = performance.now();
  navigationFromRoute = fromRoute ?? window.location.hash;
  if (trigger !== undefined) {
    navigationTrigger = trigger;
  }
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
  const to_route = props?.to_route ?? window.location.hash;
  const route_kind = parseRoute(to_route).kind;
  track("ajax_route_visible", {
    duration_ms,
    route_kind,
    ...(navigationFromRoute || props?.from_route
      ? { from_route: props?.from_route ?? navigationFromRoute }
      : {}),
    ...(props?.to_route ? { to_route: props.to_route } : {}),
    ...(navigationTrigger ? { nav_trigger: navigationTrigger } : {}),
  });
  navigationStartedAt = null;
  navigationFromRoute = null;
  navigationTrigger = null;
}

/** Once per cold boot — duration from navigation start to first shell visibility. */
export function capturePwaLaunch(duration_ms?: number): void {
  if (pwaLaunchCaptured) {
    return;
  }
  pwaLaunchCaptured = true;
  const navigationEntry = performance.getEntriesByType("navigation")[0] as
    | PerformanceNavigationTiming
    | undefined;
  track("ajax_pwa_launch", {
    duration_ms: duration_ms ?? Math.round(performance.now()),
    ...(navigationEntry
      ? {
          nav_type: navigationEntry.type,
          dom_interactive_ms: Math.round(navigationEntry.domInteractive),
        }
      : {}),
  });
}

export type PwaResumeTelemetryProps = {
  hidden_ms: number;
  resume_to_visible_ms: number;
  resume_to_cockpit_ms: number;
  resume_debounce_ms: number;
  online: boolean;
  cockpit_ok: boolean;
};

export function capturePwaResume(props: PwaResumeTelemetryProps): void {
  track("ajax_pwa_resume", props);
}

export function captureTelemetryDiagnostic(): void {
  const status = getTelemetryStatus();
  const appVersion = readAppVersion();
  track("ajax_telemetry_diagnostic", {
    initialized: status.initialized,
    standalone: status.standalone,
    ...(appVersion ? { app_version: appVersion } : {}),
  });
}
