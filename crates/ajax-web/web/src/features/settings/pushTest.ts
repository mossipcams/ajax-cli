import {
  ApiError,
  fetchPushVapidPublicKey,
  sendPushSubscribe,
  sendPushTest,
  sendPushUnsubscribe,
} from "@/shared/lib/api";
import { isStandaloneDisplay } from "@/shared/lib/telemetry";

/** Server waits this long before curl delivery so the PWA can be fully quit. */
export const PUSH_TEST_DELAY_MS = 20_000;

export type PushStatusCallback = (status: string) => void;

interface PushSubscriptionJson {
  endpoint: string;
  keys: {
    p256dh: string;
    auth: string;
  };
}

interface PushSubscriptionLike {
  toJSON(): PushSubscriptionJson;
  unsubscribe(): Promise<boolean>;
}

interface PushManagerLike {
  subscribe(options: {
    userVisibleOnly: boolean;
    applicationServerKey: Uint8Array;
  }): Promise<PushSubscriptionLike>;
  getSubscription(): Promise<PushSubscriptionLike | null>;
}

declare global {
  interface Window {
    pushManager?: PushManagerLike;
  }
}

function unsupportedMessage(): string {
  if (!isStandaloneDisplay()) {
    return "Add Ajax to the Home Screen, then enable notifications here.";
  }
  return "Declarative push is not supported in this browser.";
}

/** Drop a stale subscription when the server VAPID key rotated after restart. */
async function subscribeWithCurrentVapidKey(
  applicationServerKey: Uint8Array,
): Promise<PushSubscriptionLike> {
  // Keep `window.pushManager.subscribe` as a property chain so the install
  // allowlist string survives minify (a renamed local binding would not).
  const existing = await window.pushManager!.getSubscription();
  if (existing) {
    await existing.unsubscribe();
  }
  return window.pushManager!.subscribe({
    userVisibleOnly: true,
    applicationServerKey,
  });
}

export function urlSafeBase64ToUint8Array(base64: string): Uint8Array {
  const pad = "=".repeat((4 - (base64.length % 4)) % 4);
  const binary = atob((base64 + pad).replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

async function ensurePermission(): Promise<string | null> {
  if (typeof Notification === "undefined") {
    return null;
  }
  if (Notification.permission === "denied") {
    return "Notification permission was denied.";
  }
  if (Notification.permission === "default") {
    const permission = await Notification.requestPermission();
    if (permission !== "granted") {
      return "Notification permission was denied.";
    }
  }
  return null;
}

function subscriptionPayload(subscription: PushSubscriptionLike): PushSubscriptionJson {
  return subscription.toJSON();
}

export async function enablePushNotifications(
  onStatus: PushStatusCallback,
): Promise<{ ok: true } | { ok: false; error: string }> {
  if (!window.pushManager) {
    return { ok: false, error: unsupportedMessage() };
  }
  try {
    const permissionError = await ensurePermission();
    if (permissionError) {
      return { ok: false, error: permissionError };
    }
    onStatus("Fetching VAPID key…");
    const { public_key } = await fetchPushVapidPublicKey();
    const applicationServerKey = urlSafeBase64ToUint8Array(public_key);
    onStatus("Subscribing…");
    const subscription = await subscribeWithCurrentVapidKey(applicationServerKey);
    const payload = subscriptionPayload(subscription);
    await sendPushSubscribe({
      endpoint: payload.endpoint,
      keys: {
        p256dh: payload.keys.p256dh,
        auth: payload.keys.auth,
      },
    });
    return { ok: true };
  } catch (error) {
    return { ok: false, error: errorMessage(error) };
  }
}

export async function disablePushNotifications(
  onStatus: PushStatusCallback,
): Promise<{ ok: true } | { ok: false; error: string }> {
  try {
    onStatus("Disabling…");
    if (window.pushManager) {
      const existing = await window.pushManager.getSubscription();
      if (existing) {
        await existing.unsubscribe();
      }
    }
    // Always clear the server store so a missing local PushSubscription cannot
    // leave endpoints that keep receiving attention pushes.
    await sendPushUnsubscribe(undefined, { all: true });
    return { ok: true };
  } catch (error) {
    return { ok: false, error: errorMessage(error) };
  }
}

export async function runPushNotificationTest(
  onStatus: PushStatusCallback,
): Promise<{ ok: true } | { ok: false; error: string }> {
  if (!window.pushManager) {
    return { ok: false, error: unsupportedMessage() };
  }

  try {
    const permissionError = await ensurePermission();
    if (permissionError) {
      return { ok: false, error: permissionError };
    }

    onStatus("Fetching VAPID key…");
    const { public_key } = await fetchPushVapidPublicKey();
    const applicationServerKey = urlSafeBase64ToUint8Array(public_key);

    onStatus("Subscribing to push…");
    const subscription = await subscribeWithCurrentVapidKey(applicationServerKey);
    const payload = subscriptionPayload(subscription);
    await sendPushSubscribe({
      endpoint: payload.endpoint,
      keys: {
        p256dh: payload.keys.p256dh,
        auth: payload.keys.auth,
      },
    });

    // POST immediately with a server-side delay. A client setTimeout dies when
    // the PWA is fully closed, so the push would never be requested.
    onStatus("Scheduled — close or background the app now");
    await sendPushTest({
      endpoint: payload.endpoint,
      keys: {
        p256dh: payload.keys.p256dh,
        auth: payload.keys.auth,
      },
      delay_ms: PUSH_TEST_DELAY_MS,
    });

    return { ok: true };
  } catch (error) {
    return { ok: false, error: errorMessage(error) };
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}
