import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import routingSource from "@/features/task-workspace/taskWorkspaceRouting.ts?raw";
import appSource from "@/app/App.tsx?raw";
import TaskWorkspaceRoute from "./TaskWorkspaceRoute";

vi.mock("@/features/task-workspace/public", () => ({
  TaskWorkspace: () => null,
}));

describe("task workspace routing", () => {
  it("resolves chat vs terminal hashes from capability and preference", () => {
    expect(routingSource).toMatch(/resolveTaskWorkspaceHash/);
    expect(routingSource).toMatch(/readTaskTerminalPreferred/);
    expect(routingSource).toMatch(/sessionHash/);
    expect(routingSource).toMatch(/taskHash/);
  });

  it("routes Diff Review back through task workspace hash resolution in App", () => {
    const diffBlock = appSource.match(/<DiffReview[\s\S]*?\/>/)?.[0] ?? "";
    expect(diffBlock).toMatch(/resolveTaskWorkspaceHash\(handle/);
    expect(diffBlock).toMatch(/detailSessionCapable\(/);
  });

  it("does not announce the whole workspace outlet as a live region", () => {
    render(
      <TaskWorkspaceRoute
        kind="task"
        handle="task-1"
        orchestrationChat={false}
        detail={{ status: "loading", data: null, error: null }}
        reload={vi.fn()}
        onGo={vi.fn()}
        onBack={vi.fn()}
        onOpenDiff={vi.fn()}
      />,
    );
    expect(screen.getByTestId("outlet-task")).not.toHaveAttribute("aria-live");
  });
});
