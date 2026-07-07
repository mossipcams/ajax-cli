import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import NewTaskSheet from "./NewTaskSheet.svelte";
import newTaskSheetSource from "./NewTaskSheet.svelte?raw";
import fullscreenLayerSource from "./FullscreenLayer.svelte?raw";
import * as api from "../api";

const repos = [{ name: "web" }, { name: "api" }];

afterEach(() => vi.restoreAllMocks());

describe("NewTaskSheet", () => {
  it("exposes data-testid new-task-sheet", () => {
    const { getByTestId } = render(NewTaskSheet, { props: { repos } });
    expect(getByTestId("new-task-sheet")).toHaveAttribute("id", "new-task-sheet");
  });

  it("scrolls the sheet card internally when content exceeds the band", () => {
    expect(newTaskSheetSource).toMatch(/FullscreenLayer/);
    expect(newTaskSheetSource).not.toMatch(/--app-height|--app-top/);
    expect(newTaskSheetSource).toMatch(/\.sheet-card\s*\{[^}]*overflow-y:\s*auto/);
    expect(newTaskSheetSource).toMatch(/\.sheet-card\s*\{[^}]*max-height:\s*calc\(100% - 40px\)/);
    expect(fullscreenLayerSource).toMatch(/--app-band-top/);
    expect(fullscreenLayerSource).toMatch(/--app-band-height/);
  });

  it("offers every supported agent including opencode", () => {
    const { container } = render(NewTaskSheet, { props: { repos } });
    const options = [...container.querySelectorAll<HTMLOptionElement>("#new-task-agent option")];
    expect(options.map((option) => option.value)).toEqual([
      "codex",
      "claude",
      "cursor",
      "opencode",
    ]);
  });

  it("submits the selected opencode agent", async () => {
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const { container } = render(NewTaskSheet, { props: { repos } });
    await fireEvent.input(container.querySelector("#new-task-title-input")!, {
      target: { value: "Fix login" },
    });
    await fireEvent.change(container.querySelector("#new-task-agent")!, {
      target: { value: "opencode" },
    });
    await fireEvent.submit(container.querySelector("form")!);
    expect(spy.mock.calls[0][0].agent).toBe("opencode");
  });

  it("preselects the matching repo for the selected project", () => {
    const { container } = render(NewTaskSheet, { props: { repos, selectedProject: "api" } });
    const select = container.querySelector<HTMLSelectElement>("#new-task-repo")!;
    expect(select.value).toBe("api");
  });

  it("dismisses when the grabber is dragged down past the threshold", () => {
    const onClose = vi.fn();
    const { container } = render(NewTaskSheet, { props: { repos, onClose } });
    const grab = container.querySelector(".sheet-grab")!;
    expect(grab).not.toBeNull();

    const touch = (type: string, clientY: number) => {
      const event = new Event(type, { bubbles: true });
      Object.defineProperty(event, "touches", { value: [{ clientY }] });
      return event;
    };
    grab.dispatchEvent(touch("touchstart", 0));
    grab.dispatchEvent(touch("touchmove", 300));
    grab.dispatchEvent(new Event("touchend"));

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("rejects an empty title locally without calling the API", async () => {
    const spy = vi.spyOn(api, "startTask");
    const { container, getByText } = render(NewTaskSheet, { props: { repos } });
    await fireEvent.submit(container.querySelector("form")!);
    expect(getByText("Add a title")).toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
  });

  it("submits with a request id and applies the refreshed cockpit on success", async () => {
    const cockpit = {
      backend: { authority: "host-native", control_enabled: true },
      repos: { repos: [] },
      cards: [],
      inbox: { items: [] },
    };
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: { cockpit } });
    const onCockpit = vi.fn();
    const onClose = vi.fn();
    const { container } = render(NewTaskSheet, { props: { repos, onCockpit, onClose } });
    await fireEvent.input(container.querySelector("#new-task-title-input")!, {
      target: { value: "Fix login" },
    });
    await fireEvent.submit(container.querySelector("form")!);
    expect(spy).toHaveBeenCalledOnce();
    const arg = spy.mock.calls[0][0];
    expect(arg.title).toBe("Fix login");
    expect(arg.request_id).toBeTruthy();
    expect(onCockpit).toHaveBeenCalledWith(cockpit);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("renders a server error and keeps the sheet open", async () => {
    vi.spyOn(api, "startTask").mockResolvedValue({
      ok: false,
      response: { error: "Repo busy" },
      error: new api.ApiError("http", "Repo busy", 500),
    });
    const onClose = vi.fn();
    const { container, findByText } = render(NewTaskSheet, { props: { repos, onClose } });
    await fireEvent.input(container.querySelector("#new-task-title-input")!, {
      target: { value: "x" },
    });
    await fireEvent.submit(container.querySelector("form")!);
    expect(await findByText("Repo busy")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});
