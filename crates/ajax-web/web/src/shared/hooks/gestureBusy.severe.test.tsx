// Round 8 — HIGH-severity gesture / poll truth hunts.
// AJAX_CHAOS=1 npm run web:test -- --run src/shared/hooks/gestureBusy.severe.test.tsx

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { gestureBusyGate } from "@/shared/lib/cockpitPoll";
import { useCockpitResource } from "./useCockpitResource";
import type { BrowserCockpitView } from "@/shared/lib/types";

const chaos = process.env.AJAX_CHAOS === "1";

const fetchCockpit = vi.fn<() => Promise<BrowserCockpitView>>();

vi.mock("@/shared/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/shared/lib/api")>();
  return {
    ...actual,
    fetchCockpit: () => fetchCockpit(),
  };
});

function drainBusy() {
  while (gestureBusyGate.isBusy()) gestureBusyGate.end();
}

const fresh: BrowserCockpitView = {
  cards: [
    {
      qualified_handle: "ajax-cli/fresh",
      title: "FRESH",
      status: "running",
      actions: [],
    } as never,
  ],
} as BrowserCockpitView;

const stale: BrowserCockpitView = {
  cards: [
    {
      qualified_handle: "ajax-cli/stale",
      title: "STALE",
      status: "idle",
      actions: [],
    } as never,
  ],
} as BrowserCockpitView;

describe.runIf(chaos)("gestureBusy SEVERE cockpit truth", () => {
  beforeEach(() => {
    drainBusy();
    fetchCockpit.mockReset();
  });
  afterEach(() => {
    drainBusy();
    vi.clearAllMocks();
  });

  it("HIGH: deferred poll after mutation must not clobber fresher cockpit when gesture ends", async () => {
    // Poll starts first (in flight / parked), mutation lands FRESH, then the older
    // poll resolves — FRESH must remain (#801).
    let resolveFetch!: (value: BrowserCockpitView) => void;
    fetchCockpit.mockImplementation(
      () => new Promise((resolve) => {
        resolveFetch = resolve;
      }),
    );
    const { result } = renderHook(() => useCockpitResource());

    gestureBusyGate.begin();
    let loadDone!: Promise<void>;
    await act(async () => {
      loadDone = result.current.loadCockpit({ deferDuringGesture: true });
    });

    await act(async () => {
      result.current.applyCockpit(fresh);
    });
    await waitFor(() => expect(result.current.cockpit.data?.cards[0]?.title).toBe("FRESH"));

    await act(async () => {
      resolveFetch(stale);
      await loadDone;
    });
    expect(result.current.cockpit.data?.cards[0]?.title).toBe("FRESH");

    await act(async () => {
      gestureBusyGate.end();
      await new Promise((r) => setTimeout(r, 0));
    });

    expect(result.current.cockpit.data?.cards[0]?.title).toBe("FRESH");
  });

  it("HIGH: leaked gestureBusy begin without end must not freeze cockpit updates forever", async () => {
    fetchCockpit.mockResolvedValue(fresh);
    const { result } = renderHook(() => useCockpitResource());

    gestureBusyGate.begin(); // leak — no matching end (unmount mid-swipe hazard)

    await act(async () => {
      await result.current.loadCockpit({ deferDuringGesture: true });
    });

    // Non-deferred apply (mutation / resume) must still land.
    await act(async () => {
      result.current.applyCockpit(fresh);
    });
    expect(result.current.cockpit.data?.cards[0]?.title).toBe("FRESH");

    // Background poll while leaked-busy stays deferred — dashboard truth can
    // rot until something ends the gate. Document the freeze.
    fetchCockpit.mockResolvedValue(stale);
    await act(async () => {
      await result.current.loadCockpit({ deferDuringGesture: true });
    });
    expect(result.current.cockpit.data?.cards[0]?.title).toBe("FRESH");
    expect(gestureBusyGate.isBusy()).toBe(true);
  });
});
