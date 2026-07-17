import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import TaskList from "./TaskList.svelte";
import taskListSource from "./TaskList.svelte?raw";
import type { BrowserCockpitView } from "../types";

const NOW_SECS = Math.floor(Date.now() / 1000);

const cockpit: BrowserCockpitView = {
  backend: { authority: "host-native", control_enabled: true },
  repos: {
    repos: [
      { name: "web", attention_items: 2 },
      { name: "api", attention_items: 0 },
    ],
  },
  cards: [
    {
      id: "web/a",
      qualified_handle: "web/a",
      repo: "web",
      title: "A",
      status: "error",
      status_explanation: "CI failed",
      last_activity_unix_secs: NOW_SECS - 60,
      actions: [
        { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
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
      last_activity_unix_secs: NOW_SECS - 300,
      actions: [
        { action: "resume", label: "Resume", destructive: false, confirmation_required: false },
      ],
    },
    {
      id: "api/c",
      qualified_handle: "api/c",
      repo: "api",
      title: "C",
      status: "idle",
      last_activity_unix_secs: 0,
      actions: [],
    },
  ],
  inbox: { items: [{ task_handle: "web/a", severity: 1 }] },
};

describe("TaskList", () => {
  it("shows relative last-activity time on task rows and omits it when unset", () => {
    const { container } = render(TaskList, { props: { cockpit } });
    const rowB = container.querySelector(".task-row[data-handle='web/b']");
    expect(rowB!.textContent).toContain("5m ago");
    const rowC = container.querySelector(".task-row[data-handle='api/c']");
    expect(rowC!.textContent).not.toContain("ago");
  });

  it("renders the inbox item as a compact row with explanation and a swipe-revealed action", () => {
    const { container, getByText, queryByText } = render(TaskList, { props: { cockpit } });
    const inboxRow = container.querySelector(".task-row.is-inbox[data-handle='web/a']");
    expect(inboxRow).not.toBeNull();
    expect(getByText("CI failed")).toBeInTheDocument();
    // The first non-resume action is reachable via swipe-reveal, same affordance
    // as calm rows — no permanently-visible action-button row.
    const wrap = container.querySelector(".task-row-wrap[data-handle='web/a']");
    expect(wrap!.querySelector(".task-row-reveal")).not.toBeNull();
    expect(getByText("Fix CI")).toBeInTheDocument();
    expect(queryByText("Open")).not.toBeInTheDocument();
    expect(queryByText("Resume")).not.toBeInTheDocument();
  });

  it("renders calm task rows excluding inbox tasks", () => {
    const { container } = render(TaskList, { props: { cockpit } });
    expect(container.querySelector(".task-row[data-handle='web/b']")).not.toBeNull();
    expect(container.querySelector(".task-row[data-handle='api/c']")).not.toBeNull();
    // web/a renders inside the "Needs you" group, not the calm "Tasks" group.
    expect(container.querySelector(".group.tasks [data-handle='web/a']")).toBeNull();
    expect(container.querySelector(".group.inbox [data-handle='web/a']")).not.toBeNull();
  });

  it("shows per-repo attention counts on project pills", () => {
    const { container } = render(TaskList, { props: { cockpit } });
    const pills = [...container.querySelectorAll(".project-pill")];
    const webPill = pills.find((pill) => pill.textContent?.includes("web"))!;
    expect(webPill.querySelector(".pill-badge")).toHaveTextContent("2");
    expect(webPill).toHaveAttribute("aria-label", "web — 2 need attention");
    const apiPill = pills.find((pill) => pill.textContent?.includes("api"))!;
    expect(apiPill.querySelector(".pill-badge")).toBeNull();
  });

  it("marks the active project pill for assistive tech", () => {
    const { container } = render(TaskList, {
      props: { cockpit, selectedProject: "api" },
    });
    const pills = [...container.querySelectorAll(".project-pill")];
    const allPill = pills.find((pill) => pill.textContent?.trim().startsWith("All"))!;
    const apiPill = pills.find((pill) => pill.textContent?.includes("api"))!;
    expect(apiPill).toHaveAttribute("aria-current", "true");
    expect(allPill).not.toHaveAttribute("aria-current");
  });

  it("offers project pills and reports selection", async () => {
    const onSelectProject = vi.fn();
    const { getByText, container } = render(TaskList, { props: { cockpit, onSelectProject } });
    expect(getByText("All")).toBeInTheDocument();
    const pills = [...container.querySelectorAll(".project-pill")];
    const webPill = pills.find((pill) => pill.getAttribute("aria-label")?.startsWith("web"))!;
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

  it("empty state points at the new-task CTA", () => {
    const docsCockpit: BrowserCockpitView = {
      ...cockpit,
      repos: { repos: [...cockpit.repos.repos, { name: "docs" }] },
    };
    const { getByText: getDocsEmpty } = render(TaskList, {
      props: { cockpit: docsCockpit, selectedProject: "docs" },
    });
    expect(getDocsEmpty("No tasks in docs yet — start one below.")).toBeInTheDocument();

    const emptyCockpit: BrowserCockpitView = {
      ...cockpit,
      cards: [],
      inbox: { items: [] },
    };
    const { getByText: getAllEmpty } = render(TaskList, {
      props: { cockpit: emptyCockpit },
    });
    expect(getAllEmpty("All quiet — start a new task below.")).toBeInTheDocument();
  });

  it("opens a task when a row is tapped", async () => {
    const onOpenTask = vi.fn();
    const { container } = render(TaskList, { props: { cockpit, onOpenTask } });
    await fireEvent.click(container.querySelector(".task-row[data-handle='api/c']")!);
    expect(onOpenTask).toHaveBeenCalledWith("api/c");
  });

  it("opens an inbox task when the row is tapped", async () => {
    const onOpenTask = vi.fn();
    const { container } = render(TaskList, { props: { cockpit, onOpenTask } });
    await fireEvent.click(container.querySelector(".task-row[data-handle='web/a']")!);
    expect(onOpenTask).toHaveBeenCalledWith("web/a");
  });

  it("does not reveal resume as a calm-row action", () => {
    const { container } = render(TaskList, { props: { cockpit } });
    const wrap = container.querySelector(".task-row-wrap[data-handle='web/b']");
    expect(wrap).not.toBeNull();
    expect(wrap!.querySelector(".task-row-reveal")).toBeNull();
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
          last_activity_unix_secs: 0,
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

  it("renders no reveal for a calm row without non-resume actions", () => {
    const { container } = render(TaskList, { props: { cockpit } });
    const wrap = container.querySelector(".task-row-wrap[data-handle='api/c']");
    expect(wrap).not.toBeNull();
    expect(wrap!.querySelector(".task-row-reveal")).toBeNull();
  });

  it("keeps is-inbox semantics without a left border stripe", () => {
    const inboxRule =
      taskListSource.match(/\.task-row\.is-inbox\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(inboxRule).not.toMatch(/border-left/);
    expect(inboxRule).not.toMatch(/padding-left:\s*calc/);
  });

  it("uses accent for the active project pill and warn for attention badges", () => {
    const activePillRule =
      taskListSource.match(/\.project-pill\.is-active\s*\{([^}]*)\}/)?.[1] ?? "";
    const pillBadgeRule = taskListSource.match(/\.pill-badge\s*\{([^}]*)\}/)?.[1] ?? "";
    const attentionTitleRule =
      taskListSource.match(/\.section-head\.attention \.section-head-title\s*\{([^}]*)\}/)?.[1] ??
      "";
    const attentionCountRule =
      taskListSource.match(/\.section-head\.attention \.section-head-count\s*\{([^}]*)\}/)?.[1] ??
      "";

    expect(activePillRule).toMatch(/var\(--accent(?:-bright|-deep)?\)/);
    expect(activePillRule).not.toMatch(/var\(--warn/);
    expect(pillBadgeRule).toMatch(/var\(--warn/);
    expect(attentionTitleRule).toMatch(/var\(--warn/);
    expect(attentionCountRule).toMatch(/var\(--warn/);
  });
});
