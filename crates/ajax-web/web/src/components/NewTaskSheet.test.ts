import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import NewTaskSheet from "./NewTaskSheet.svelte";
import * as api from "../api";

const repos = [{ name: "web" }, { name: "api" }];

afterEach(() => vi.restoreAllMocks());

describe("NewTaskSheet", () => {
  it("preselects the matching repo for the selected project", () => {
    const { container } = render(NewTaskSheet, { props: { repos, selectedProject: "api" } });
    const select = container.querySelector<HTMLSelectElement>("#new-task-repo")!;
    expect(select.value).toBe("api");
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
