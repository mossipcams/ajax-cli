import type { ChatHeadView } from "./headView";

const baseView: ChatHeadView = {
  state: "idle",
  tone: "idle",
  connected: true,
  activityAgeMs: 0,
  decision: null,
  hasActivity: false,
  usage: null,
  turnUsage: null,
  taskAttention: null,
  attentionText: null,
  showHeadLine: true,
};

export function initialHeadViewForTests(overrides: Partial<ChatHeadView> = {}): ChatHeadView {
  return { ...baseView, ...overrides };
}
