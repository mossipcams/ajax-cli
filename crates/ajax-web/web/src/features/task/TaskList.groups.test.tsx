import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
import TaskList from "./TaskList";
import type { BrowserCockpitView, BrowserTaskCard } from "@/shared/lib/types";

const NOW_SECS = Math.floor(Date.now() / 1000);

function card(
  handle: string,
  status: string,
  attention?: BrowserTaskCard["attention"],
): BrowserTaskCard {
  return {
    id: handle,
    qualified_handle: handle,
    repo: handle.split("/")[0],
    title: handle,
    status,
    attention,
    last_activity_unix_secs: NOW_SECS - 60,
    actions: [],
  } as BrowserTaskCard;
}

function cockpitWith(cards: BrowserTaskCard[]): BrowserCockpitView {
  return {
    backend: { authority: "host-native", control_enabled: true },
    repos: { repos: [] },
    inbox: { items: [] },
    cards,
  } as unknown as BrowserCockpitView;
}

function listSequence(): string[] {
  const headings = screen
    .queryAllByRole("heading")
    .map((el) => ({ el, label: `heading:${el.textContent}` }));
  const rows = screen
    .queryAllByRole("button", { name: /^(web|api)\// })
    .map((el) => ({ el, label: `row:${el.getAttribute("data-handle")}` }));
  return [...headings, ...rows]
    .sort((a, b) =>
      a.el.compareDocumentPosition(b.el) & Node.DOCUMENT_POSITION_FOLLOWING
        ? -1
        : 1,
    )
    .map((item) => item.label);
}

describe("TaskList attention groups", () => {
  it("puts needs-you and review rows under Needs you and the rest under Other tasks, keeping sort order in each group", () => {
    render(
      <TaskList
        cockpit={cockpitWith([
          card("web/review", "idle", "review"),
          card("web/error", "error", "needs-you"),
          card("web/run", "running", "active"),
          card("api/wait", "waiting", "needs-you"),
          card("web/wait-active", "waiting", "active"),
          card("api/idle", "idle", "idle"),
        ])}
      />,
    );

    expect(listSequence()).toEqual([
      "heading:Needs you",
      "row:api/wait",
      "row:web/error",
      "row:web/review",
      "heading:Other tasks",
      "row:web/run",
      "row:web/wait-active",
      "row:api/idle",
    ]);

    const tasks = screen.getByRole("region", { name: "Tasks" });
    expect(
      within(tasks).getByRole("heading", { name: "Needs you", level: 2 }),
    ).toBeInTheDocument();
    expect(
      within(tasks).getByRole("heading", { name: "Other tasks", level: 2 }),
    ).toBeInTheDocument();
  });

  it("keeps a waiting task with active attention under Other tasks", () => {
    render(
      <TaskList
        cockpit={cockpitWith([
          card("api/wait", "waiting", "needs-you"),
          card("web/wait-active", "waiting", "active"),
        ])}
      />,
    );

    expect(listSequence()).toEqual([
      "heading:Needs you",
      "row:api/wait",
      "heading:Other tasks",
      "row:web/wait-active",
    ]);
  });

  it("puts cards without an attention field under Other tasks", () => {
    render(
      <TaskList
        cockpit={cockpitWith([
          card("web/plain", "running"),
          card("api/wait", "waiting", "needs-you"),
        ])}
      />,
    );

    expect(listSequence()).toEqual([
      "heading:Needs you",
      "row:api/wait",
      "heading:Other tasks",
      "row:web/plain",
    ]);
  });

  it("omits Other tasks when every row needs the operator", () => {
    render(
      <TaskList
        cockpit={cockpitWith([
          card("web/error", "error", "needs-you"),
          card("api/review", "idle", "review"),
        ])}
      />,
    );

    expect(
      screen.getByRole("heading", { name: "Needs you" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Other tasks" })).toBeNull();
    expect(listSequence()).toEqual([
      "heading:Needs you",
      "row:web/error",
      "row:api/review",
    ]);
  });

  it("shows no headings when no row needs the operator", () => {
    render(
      <TaskList
        cockpit={cockpitWith([
          card("web/run", "running", "active"),
          card("api/idle", "idle", "idle"),
        ])}
      />,
    );

    expect(screen.queryByRole("heading")).toBeNull();
    expect(listSequence()).toEqual(["row:web/run", "row:api/idle"]);
  });
});
