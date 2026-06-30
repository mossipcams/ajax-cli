// Phase 6.2: Contract fixture parity. These tests verify that the representative
// fixture JSON files satisfy the TypeScript boundary guards and DTO shape.
// If Rust changes a serialized field name or enum casing these tests will fail,
// prompting a synchronized types.ts update.
import { describe, it, expect } from "vitest";
import {
  assertCockpit,
  assertDetail,
  assertOperationResponse,
} from "./contracts";

import cockpit from "./fixtures/cockpit.json";
import taskDetail from "./fixtures/task-detail.json";
import operation from "./fixtures/operation.json";

describe("cockpit fixture", () => {
  it("passes boundary guard without throwing", () => {
    const view = assertCockpit(cockpit);
    expect(view.cards).toHaveLength(1);
  });

  it("has explicit repo identity on every card", () => {
    const view = assertCockpit(cockpit);
    for (const card of view.cards) {
      expect(typeof card.repo).toBe("string");
      expect(card.repo.length).toBeGreaterThan(0);
    }
  });

  it("actions carry no status field", () => {
    const view = assertCockpit(cockpit);
    for (const card of view.cards) {
      for (const action of card.actions) {
        expect((action as unknown as Record<string, unknown>).status).toBeUndefined();
      }
    }
  });

  it("destructive action has confirmation_required", () => {
    const view = assertCockpit(cockpit);
    const drop = view.cards[0].actions.find((a) => a.action === "drop");
    expect(drop?.destructive).toBe(true);
    expect(drop?.confirmation_required).toBe(true);
  });

  it("inbox items reference existing card handles", () => {
    const handles = new Set(cockpit.cards.map((c) => c.qualified_handle));
    for (const item of cockpit.inbox.items) {
      expect(handles.has(item.task_handle)).toBe(true);
    }
  });

  it("omits deleted or ghost task records from the cockpit projection", () => {
    const view = assertCockpit(cockpit);
    const cardHandles = new Set(view.cards.map((card) => card.qualified_handle));

    for (const card of view.cards) {
      expect(card.qualified_handle.length).toBeGreaterThan(0);
      expect(card.title.length).toBeGreaterThan(0);
      expect(card.qualified_handle).not.toMatch(/removed|ghost|deleted/i);
    }

    for (const item of view.inbox.items) {
      expect(cardHandles.has(item.task_handle)).toBe(true);
      expect(item.task_handle).not.toMatch(/removed|ghost|deleted/i);
    }
  });
});

describe("task-detail fixture", () => {
  it("has all required top-level fields", () => {
    const d = assertDetail(taskDetail);
    expect(typeof d.qualified_handle).toBe("string");
    expect(typeof d.repo).toBe("string");
    expect(typeof d.status).toBe("string");
    expect(Array.isArray(d.actions)).toBe(true);
    expect(typeof d.created_unix_secs).toBe("number");
    expect(typeof d.last_activity_unix_secs).toBe("number");
  });

  it("actions carry server-provided labels", () => {
    const d = assertDetail(taskDetail);
    for (const action of d.actions) {
      expect(action.label.length).toBeGreaterThan(0);
    }
  });

  it("agent_attempts is an array of attempts", () => {
    const d = assertDetail(taskDetail);
    expect(Array.isArray(d.agent_attempts)).toBe(true);
    expect(d.agent_attempts[0].started_unix_secs).toBeTypeOf("number");
  });
});

describe("operation response fixture", () => {
  it("refreshed cockpit passes boundary guard", () => {
    const resp = assertOperationResponse(operation);
    if (resp.cockpit) {
      const view = assertCockpit(resp.cockpit);
      expect(view.cards).toHaveLength(1);
    }
  });

  it("output is a string when present", () => {
    const resp = assertOperationResponse(operation);
    if (resp.output != null) {
      expect(typeof resp.output).toBe("string");
    }
  });
});
