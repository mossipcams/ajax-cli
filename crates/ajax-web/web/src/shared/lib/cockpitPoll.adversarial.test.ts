// Opt-in. AJAX_CHAOS=1 npm run web:test -- --run src/shared/lib/cockpitPoll.adversarial.test.ts

import { describe, it, expect } from "vitest";
import { createCockpitApplyGate, createInFlightGuard } from "./cockpitPoll";
import type { BrowserCockpitView } from "./types";

const chaos = process.env.AJAX_CHAOS === "1";

function view(cards: Array<{ id: string; status: string }>): BrowserCockpitView {
  return {
    backend: { authority: "host-native", control_enabled: true, warning: null },
    repos: { repos: [] },
    cards: cards.map((card) => ({
      id: card.id,
      qualified_handle: card.id,
      repo: "web",
      title: card.id,
      status: card.status,
      status_explanation: null,
      actions: [],
    })),
    inbox: { items: [] },
  } as BrowserCockpitView;
}

describe.runIf(chaos)("cockpitPoll adversarial", () => {
  it("does not apply an older poll projection after a newer mutation projection", () => {
    const gate = createCockpitApplyGate();
    const mutation = view([{ id: "web/a", status: "running" }]);
    const olderPoll = view([{ id: "web/a", status: "waiting" }]);

    const startedAt = gate.pollGeneration();
    gate.noteMutation();
    expect(gate.applyIfChanged(mutation)).toBe(true);
    expect(gate.applyPollIfChanged(olderPoll, startedAt)).toBe(false);
  });

  it("coalesces bursty trailing runs to a single trailing flight", async () => {
    const guard = createInFlightGuard();
    let runs = 0;
    const slow = () =>
      new Promise<number>((resolve) => {
        runs += 1;
        setTimeout(() => resolve(runs), 30);
      });
    const first = guard.run(slow);
    void guard.run(slow, { trailing: true });
    void guard.run(slow, { trailing: true });
    void guard.run(slow, { trailing: true });
    await first;
    await new Promise((r) => setTimeout(r, 80));
    // One in-flight + one trailing coalesce, not 1+3.
    expect(runs).toBeLessThanOrEqual(2);
  });
});

describe("cockpitPoll adversarial gate", () => {
  it("documents AJAX_CHAOS opt-in", () => {
    expect(typeof chaos).toBe("boolean");
  });
});
