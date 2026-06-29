import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TaskList from "./TaskList.svelte";
import type { BrowserCockpitView } from "../types";

const cockpit: BrowserCockpitView = {
  backend: { authority: "host-native", control_enabled: true },
  repos: { repos: [{ name: "web" }, { name: "api" }] },
  cards: [
    {
      id: "web/a",
      qualified_handle: "web/a",
      repo: "web",
      title: "A",
      status: "error",
      status_explanation: "CI failed",
      actions: [
        { action: "fix-ci", label: "Fix CI", destructive: false, confirmation_required: false },
        { action: "drop", label: "Drop", destructive: true, confirmation_required: true },
      ],
    },
    {
      id: "web/b",
      qualified_handle: "web/b",
      repo: "web",
      title: "B",
      status: "running",
      status_explanation: "Agent working",
      actions: [],
    },
    {
      id: "api/c",
      qualified_handle: "api/c",
      repo: "api",
      title: "C",
      status: "idle",
      actions: [],
    },
  ],
  inbox: { items: [{ task_handle: "web/a", severity: 1 }] },
};

describe("TaskList", () => {
  it("renders the inbox card with explanation and ordered actions", () => {
    const { container, getByText } = render(TaskList, { props: { cockpit } });
    const inboxCard = container.querySelector(".inbox-card[data-handle='web/a']");
    expect(inboxCard).not.toBeNull();
    expect(getByText("CI failed")).toBeInTheDocument();
    expect(getByText("Fix CI")).toBeInTheDocument();
  });

  it("renders calm task rows excluding inbox tasks", () => {
    const { container } = render(TaskList, { props: { cockpit } });
    expect(container.querySelector(".task-row[data-handle='web/b']")).not.toBeNull();
    expect(container.querySelector(".task-row[data-handle='api/c']")).not.toBeNull();
    // web/a is in the inbox, not the calm list.
    expect(container.querySelector(".task-row[data-handle='web/a']")).toBeNull();
  });

  it("offers project pills and reports selection", async () => {
    const onSelectProject = vi.fn();
    const { getByText, container } = render(TaskList, { props: { cockpit, onSelectProject } });
    expect(getByText("All")).toBeInTheDocument();
    const pills = [...container.querySelectorAll(".project-pill")];
    const webPill = pills.find((pill) => pill.textContent?.trim() === "web")!;
    await fireEvent.click(webPill);
    expect(onSelectProject).toHaveBeenCalledWith("web");
  });

  it("filters by the selected project", () => {
    const { container } = render(TaskList, {
      props: { cockpit, selectedProject: "api" },
    });
    expect(container.querySelector(".task-row[data-handle='api/c']")).not.toBeNull();
    expect(container.querySelector(".task-row[data-handle='web/b']")).toBeNull();
  });

  it("opens a task when a row is tapped", async () => {
    const onOpenTask = vi.fn();
    const { container } = render(TaskList, { props: { cockpit, onOpenTask } });
    await fireEvent.click(container.querySelector(".task-row[data-handle='api/c']")!);
    expect(onOpenTask).toHaveBeenCalledWith("api/c");
  });

  it("reveals a swipe action behind a calm row that has actions", () => {
    const withAction: BrowserCockpitView = {
      ...cockpit,
      cards: [
        {
          id: "web/b",
          qualified_handle: "web/b",
          repo: "web",
          title: "B",
          status: "idle",
          actions: [
            { action: "review", label: "Review", destructive: false, confirmation_required: false },
          ],
        },
      ],
      inbox: { items: [] },
    };
    const { container } = render(TaskList, { props: { cockpit: withAction } });
    const wrap = container.querySelector(".task-row-wrap[data-handle='web/b']");
    expect(wrap).not.toBeNull();
    expect(wrap!.querySelector(".task-row-reveal")).not.toBeNull();
    // The row itself must remain present and tappable.
    expect(container.querySelector(".task-row[data-handle='web/b']")).not.toBeNull();
  });

  it("renders no reveal for a calm row without actions", () => {
    const { container } = render(TaskList, { props: { cockpit } });
    const wrap = container.querySelector(".task-row-wrap[data-handle='api/c']");
    expect(wrap).not.toBeNull();
    expect(wrap!.querySelector(".task-row-reveal")).toBeNull();
  });
});
