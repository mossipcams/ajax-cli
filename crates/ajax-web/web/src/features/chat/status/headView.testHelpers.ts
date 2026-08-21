import type { ChatHeadView } from "./headView";

const baseView: ChatHeadView = {
  state: "idle",
  tone: "idle",
  connected: true,
  activityAgeMs: 0,
  decision: null,
  tool: null,
  planStep: null,
  thoughtSnippet: null,
  usage: null,
  turnUsage: null,
  taskAttention: null,
  attentionText: null,
  showHeadLine: true,
};

export function initialHeadViewForTests(overrides: Partial<ChatHeadView> = {}): ChatHeadView {
  return { ...baseView, ...overrides };
}
