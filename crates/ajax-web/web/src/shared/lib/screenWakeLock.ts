/** Screen Wake Lock — keep the device awake while Cockpit is foreground-visible. */

export interface WakeLockSentinel {
  release(): Promise<void>;
  addEventListener(type: "release", listener: () => void): void;
  removeEventListener(type: "release", listener: () => void): void;
}

export interface ScreenWakeLockPlatform {
  isVisible(): boolean;
  requestWakeLock(): Promise<WakeLockSentinel>;
  addVisibilityListener(handler: () => void): () => void;
  addUserGestureListener(handler: () => void): () => void;
}

const USER_GESTURE_EVENTS = ["pointerdown", "touchstart", "keydown", "click"] as const;

export function createBrowserScreenWakeLockPlatform(): ScreenWakeLockPlatform | null {
  if (typeof document === "undefined" || typeof navigator === "undefined") {
    return null;
  }
  const wakeLock = navigator.wakeLock;
  if (!wakeLock || typeof wakeLock.request !== "function") {
    return null;
  }
  return {
    isVisible: () => document.visibilityState === "visible",
    requestWakeLock: () => wakeLock.request("screen"),
    addVisibilityListener(handler) {
      document.addEventListener("visibilitychange", handler);
      return () => document.removeEventListener("visibilitychange", handler);
    },
    addUserGestureListener(handler) {
      for (const type of USER_GESTURE_EVENTS) {
        document.addEventListener(type, handler, { capture: true, passive: true });
      }
      return () => {
        for (const type of USER_GESTURE_EVENTS) {
          document.removeEventListener(type, handler, { capture: true });
        }
      };
    },
  };
}

/**
 * Request a screen wake lock while the page is visible; release when hidden.
 * Fail-open when unsupported or denied. Re-acquire after visibility resume and
 * on the next user gesture when the platform requires activation.
 */
export function setupScreenWakeLock(
  platformOrNull: ScreenWakeLockPlatform | null = createBrowserScreenWakeLockPlatform(),
): () => void {
  if (platformOrNull === null) {
    return () => {};
  }
  const platform: ScreenWakeLockPlatform = platformOrNull;

  let sentinel: WakeLockSentinel | null = null;
  let acquireInFlight: Promise<void> | null = null;

  const onSentinelRelease = () => {
    sentinel = null;
    if (platform.isVisible()) {
      void tryAcquire();
    }
  };

  async function releaseWakeLock(): Promise<void> {
    const current = sentinel;
    sentinel = null;
    if (!current) return;
    current.removeEventListener("release", onSentinelRelease);
    try {
      await current.release();
    } catch {
      // Fail open — Cockpit still works without the lock.
    }
  }

  async function tryAcquire(): Promise<void> {
    if (!platform.isVisible() || sentinel) return;
    if (acquireInFlight) {
      await acquireInFlight;
      return;
    }
    acquireInFlight = (async () => {
      try {
        const next = await platform.requestWakeLock();
        if (!platform.isVisible()) {
          try {
            await next.release();
          } catch {
            // Fail open.
          }
          return;
        }
        sentinel = next;
        next.addEventListener("release", onSentinelRelease);
      } catch {
        // Unsupported, denied, low battery, or missing user gesture — fail open.
      } finally {
        acquireInFlight = null;
      }
    })();
    await acquireInFlight;
  }

  const onVisibilityChange = () => {
    if (platform.isVisible()) {
      void tryAcquire();
      return;
    }
    void releaseWakeLock();
  };

  const onUserGesture = () => {
    if (platform.isVisible()) {
      void tryAcquire();
    }
  };

  const removeVisibility = platform.addVisibilityListener(onVisibilityChange);
  const removeGestures = platform.addUserGestureListener(onUserGesture);

  if (platform.isVisible()) {
    void tryAcquire();
  }

  return () => {
    removeVisibility();
    removeGestures();
    void releaseWakeLock();
  };
}
