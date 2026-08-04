import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as api from "@/shared/lib/api";
import * as telemetry from "@/shared/lib/telemetry";
import {
  PUSH_TEST_DELAY_MS,
  runPushNotificationTest,
  enablePushNotifications,
  disablePushNotifications,
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
    vi.spyOn(api, "fetchPushVapidPublicKey").mockResolvedValue({
      public_key: "AQID",
    });
    vi.spyOn(api, "sendPushSubscribe").mockResolvedValue();
    vi.spyOn(api, "sendPushTest").mockResolvedValue();
    vi.spyOn(telemetry, "isStandaloneDisplay").mockReturnValue(true);
    vi.stubGlobal("Notification", {
      permission: "granted",
      requestPermission: vi.fn().mockResolvedValue("granted"),
    });
  });

  afterEach(() => {
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

  it("prompts for Home Screen when not installed", async () => {
    vi.mocked(telemetry.isStandaloneDisplay).mockReturnValue(false);
    const result = await runPushNotificationTest(vi.fn());
    expect(result).toEqual({
      ok: false,
      error: "Add Ajax to the Home Screen, then enable notifications here.",
    });
  });

  it("posts the subscription immediately with a server-side delay", async () => {
    const { subscribe } = installPushManager();
    const statuses: string[] = [];

    const result = await runPushNotificationTest((status) => statuses.push(status));

    expect(subscribe).toHaveBeenCalledWith({
      userVisibleOnly: true,
      applicationServerKey: urlSafeBase64ToUint8Array("AQID"),
    });
    expect(statuses).toContain("Scheduled — close or background the app now");
    expect(api.fetchPushVapidPublicKey).toHaveBeenCalledOnce();
    expect(api.sendPushSubscribe).toHaveBeenCalledWith(mockSubscriptionPayload);
    expect(api.sendPushTest).toHaveBeenCalledWith({
      ...mockSubscriptionPayload,
      delay_ms: PUSH_TEST_DELAY_MS,
    });
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

    const result = await runPushNotificationTest(vi.fn());
    expect(unsubscribe).toHaveBeenCalledOnce();
    expect(getSubscription).toHaveBeenCalledOnce();
    expect(subscribe).toHaveBeenCalledOnce();
    expect(result).toEqual({ ok: true });
  });

  it("requests notification permission when needed", async () => {
    const requestPermission = vi.fn().mockResolvedValue("granted");
    vi.stubGlobal("Notification", { permission: "default", requestPermission });
    installPushManager();

    const result = await runPushNotificationTest(vi.fn());

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

    const result = await runPushNotificationTest(vi.fn());

    expect(result).toEqual({ ok: false, error: "HTTP 502" });
  });
});

describe("enablePushNotifications", () => {
  beforeEach(() => {
    vi.spyOn(api, "fetchPushVapidPublicKey").mockResolvedValue({ public_key: "AQID" });
    vi.spyOn(api, "sendPushSubscribe").mockResolvedValue();
    vi.spyOn(telemetry, "isStandaloneDisplay").mockReturnValue(true);
    vi.stubGlobal("Notification", {
      permission: "granted",
      requestPermission: vi.fn().mockResolvedValue("granted"),
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("persists the subscription on the server", async () => {
    installPushManager();
    const result = await enablePushNotifications(vi.fn());
    expect(api.sendPushSubscribe).toHaveBeenCalledWith(mockSubscriptionPayload);
    expect(result).toEqual({ ok: true });
  });
});

describe("disablePushNotifications", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("clears the server store even without a local subscription", async () => {
    vi.spyOn(api, "sendPushUnsubscribe").mockResolvedValue();
    vi.stubGlobal("pushManager", {
      subscribe: vi.fn(),
      getSubscription: vi.fn().mockResolvedValue(null),
    });

    const result = await disablePushNotifications(vi.fn());

    expect(api.sendPushUnsubscribe).toHaveBeenCalledWith(undefined, { all: true });
    expect(result).toEqual({ ok: true });
  });
});
