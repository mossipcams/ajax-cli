import { ApiError, fetchPushVapidPublicKey, sendPushTest } from "@/shared/lib/api";

export const PUSH_TEST_DELAY_MS = 20_000;

export type PushTestStatusCallback = (status: string) => void;

interface PushSubscriptionJson {
  endpoint: string;
  keys: {
    p256dh: string;
    auth: string;
  };
}

interface PushSubscriptionLike {
  toJSON(): PushSubscriptionJson;
}

declare global {
  interface Window {
    pushManager?: {
      subscribe(options: {
        userVisibleOnly: boolean;
        applicationServerKey: Uint8Array;
      }): Promise<PushSubscriptionLike>;
    };
  }
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

export async function runPushNotificationTest(
  onStatus: PushTestStatusCallback,
): Promise<{ ok: true } | { ok: false; error: string }> {
  if (!window.pushManager) {
    return {
      ok: false,
      error: "Declarative push is not supported in this browser.",
    };
  }

  try {
    if (typeof Notification !== "undefined") {
      if (Notification.permission === "denied") {
        return { ok: false, error: "Notification permission was denied." };
      }
      if (Notification.permission === "default") {
        const permission = await Notification.requestPermission();
        if (permission !== "granted") {
          return { ok: false, error: "Notification permission was denied." };
        }
      }
    }

    onStatus("Fetching VAPID key…");
    const { public_key } = await fetchPushVapidPublicKey();
    const applicationServerKey = urlSafeBase64ToUint8Array(public_key);

    onStatus("Subscribing to push…");
    const subscription = await window.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey,
    });

    onStatus("Sending in 20s… background the app now");
    await new Promise((resolve) => setTimeout(resolve, PUSH_TEST_DELAY_MS));

    const payload = subscription.toJSON();
    await sendPushTest({
      endpoint: payload.endpoint,
      keys: {
        p256dh: payload.keys.p256dh,
        auth: payload.keys.auth,
      },
    });

    return { ok: true };
  } catch (error) {
    const message =
      error instanceof ApiError
        ? error.message
        : error instanceof Error
          ? error.message
          : String(error);
    return { ok: false, error: message };
  }
}
