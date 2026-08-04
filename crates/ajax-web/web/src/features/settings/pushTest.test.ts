import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as api from "@/shared/lib/api";
import {
  PUSH_TEST_DELAY_MS,
  runPushNotificationTest,
  urlSafeBase64ToUint8Array,
} from "./pushTest";

const mockSubscriptionPayload = {
  endpoint: "https://push.example/messages/1",
  keys: { p256dh: "p256dh-key", auth: "auth-key" },
};

function installPushManager(
  subscribe = vi.fn().mockResolvedValue({
    toJSON: () => mockSubscriptionPayload,
    unsubscribe: vi.fn().mockResolvedValue(true),
  }),
  getSubscription = vi.fn().mockResolvedValue(null),
) {
  vi.stubGlobal("pushManager", { subscribe, getSubscription });
  return { subscribe, getSubscription };
}

describe("urlSafeBase64ToUint8Array", () => {
  it("decodes url-safe base64 without padding", () => {
    const bytes = urlSafeBase64ToUint8Array("AQID");
    expect(Array.from(bytes)).toEqual([1, 2, 3]);
  });
});

describe("runPushNotificationTest", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(api, "fetchPushVapidPublicKey").mockResolvedValue({
      public_key: "AQID",
    });
    vi.spyOn(api, "sendPushTest").mockResolvedValue();
    vi.stubGlobal("Notification", {
      permission: "granted",
      requestPermission: vi.fn().mockResolvedValue("granted"),
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("returns unsupported when pushManager is missing", async () => {
    const result = await runPushNotificationTest(vi.fn());
    expect(result).toEqual({
      ok: false,
      error: "Declarative push is not supported in this browser.",
    });
  });

  it("waits 20s then posts the subscription", async () => {
    const { subscribe } = installPushManager();
    const statuses: string[] = [];

    const run = runPushNotificationTest((status) => statuses.push(status));
    await vi.waitFor(() => expect(subscribe).toHaveBeenCalledOnce());
    expect(subscribe).toHaveBeenCalledWith({
      userVisibleOnly: true,
      applicationServerKey: urlSafeBase64ToUint8Array("AQID"),
    });
    expect(statuses).toContain("Sending in 20s… background the app now");
    expect(api.sendPushTest).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(PUSH_TEST_DELAY_MS);
    const result = await run;

    expect(api.fetchPushVapidPublicKey).toHaveBeenCalledOnce();
    expect(api.sendPushTest).toHaveBeenCalledWith(mockSubscriptionPayload);
    expect(result).toEqual({ ok: true });
  });

  it("unsubscribes a stale subscription before resubscribing", async () => {
    const unsubscribe = vi.fn().mockResolvedValue(true);
    const stale = {
      toJSON: () => mockSubscriptionPayload,
      unsubscribe,
    };
    const subscribe = vi.fn().mockResolvedValue({
      toJSON: () => mockSubscriptionPayload,
      unsubscribe: vi.fn().mockResolvedValue(true),
    });
    const getSubscription = vi.fn().mockResolvedValue(stale);
    vi.stubGlobal("pushManager", { subscribe, getSubscription });

    const run = runPushNotificationTest(vi.fn());
    await vi.waitFor(() => expect(subscribe).toHaveBeenCalledOnce());
    expect(unsubscribe).toHaveBeenCalledOnce();
    expect(getSubscription).toHaveBeenCalledOnce();

    await vi.advanceTimersByTimeAsync(PUSH_TEST_DELAY_MS);
    const result = await run;
    expect(result).toEqual({ ok: true });
  });

  it("requests notification permission when needed", async () => {
    const requestPermission = vi.fn().mockResolvedValue("granted");
    vi.stubGlobal("Notification", { permission: "default", requestPermission });
    installPushManager();

    const run = runPushNotificationTest(vi.fn());
    await vi.advanceTimersByTimeAsync(PUSH_TEST_DELAY_MS);
    const result = await run;

    expect(requestPermission).toHaveBeenCalledOnce();
    expect(result).toEqual({ ok: true });
  });

  it("returns a clear error when permission is denied", async () => {
    vi.stubGlobal("Notification", {
      permission: "default",
      requestPermission: vi.fn().mockResolvedValue("denied"),
    });
    installPushManager();

    const result = await runPushNotificationTest(vi.fn());
    expect(result).toEqual({
      ok: false,
      error: "Notification permission was denied.",
    });
  });

  it("surfaces API errors", async () => {
    installPushManager();
    vi.mocked(api.sendPushTest).mockRejectedValue(new api.ApiError("http", "HTTP 502"));

    const run = runPushNotificationTest(vi.fn());
    await vi.advanceTimersByTimeAsync(PUSH_TEST_DELAY_MS);
    const result = await run;

    expect(result).toEqual({ ok: false, error: "HTTP 502" });
  });
});
