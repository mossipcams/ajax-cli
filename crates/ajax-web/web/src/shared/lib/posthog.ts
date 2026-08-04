import posthog from "posthog-js";

/** PostHog Cloud US project token (browser write key). */
export const POSTHOG_PROJECT_KEY =
  "phc_uQFMpY3C9L9Dj4wLqudjNyJVBwAdCisMyUkZ6EqhxWxB";

const POSTHOG_API_HOST = "https://us.i.posthog.com";
const POSTHOG_DEFAULTS = "2026-05-30";

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

const WEB_VITALS_METRICS = ["LCP", "CLS", "FCP", "INP"] as const;

let initialized = false;

export function isPostHogInitialized(): boolean {
  return initialized;
}

/** Test seam: reset module init guard between unit tests. */
export function resetPostHogForTests(): void {
  initialized = false;
}

export function readAppVersion(): string | undefined {
  const version = document
    .querySelector('meta[name="ajax-app-version"]')
    ?.getAttribute("content")
    ?.trim();
  if (!version || version === "__AJAX_APP_VERSION__") {
    return undefined;
  }
  return version;
}

export function posthogDistinctId(): string {
  return `ajax:${window.location.hostname}`;
}

export function initPostHog(): void {
  if (initialized) {
    return;
  }

  try {
    posthog.init(POSTHOG_PROJECT_KEY, {
      api_host: POSTHOG_API_HOST,
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
    posthog.identify(posthogDistinctId(), {
      host: window.location.hostname,
      origin: window.location.origin,
      ...(appVersion ? { app_version: appVersion } : {}),
    });

    initialized = true;
  } catch (error) {
    console.warn("[ajax] PostHog init failed", error);
  }
}

export type TelemetryProps = Record<
  string,
  string | number | boolean | null | undefined
>;

/** Capture a custom event when PostHog initialized; no-op otherwise. */
export function captureEvent(event: string, properties?: TelemetryProps): void {
  if (!initialized) {
    return;
  }
  try {
    posthog.capture(event, properties);
  } catch (error) {
    console.warn("[ajax] PostHog capture failed", error);
  }
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
  captureEvent("ajax_tap_to_feedback", {
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
  captureEvent("ajax_tap_to_operation_complete", {
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

export function captureSwipe(props: {
  direction: "left" | "right";
  duration_ms: number;
  from_route?: string;
  to_route?: string;
}): void {
  captureEvent("ajax_swipe", {
    ...props,
    committed: true,
  });
}
