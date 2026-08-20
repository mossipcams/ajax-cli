import { describe, it, expect } from "vitest";
import routingSource from "@/features/task-workspace/taskWorkspaceRouting.ts?raw";
import appSource from "@/app/App.tsx?raw";

describe("task workspace routing", () => {
  it("resolves chat vs terminal hashes from capability and preference", () => {
    expect(routingSource).toMatch(/resolveTaskWorkspaceHash/);
    expect(routingSource).toMatch(/readTaskTerminalPreferred/);
    expect(routingSource).toMatch(/sessionHash/);
    expect(routingSource).toMatch(/taskHash/);
  });

  it("routes Diff Review back through task workspace hash resolution in App", () => {
    const diffBlock = appSource.match(/<DiffReview[\s\S]*?\/>/)?.[0] ?? "";
    expect(diffBlock).toMatch(/resolveTaskWorkspaceHash\(route\.handle/);
    expect(diffBlock).toMatch(/detailSessionCapable\(/);
  });
});
