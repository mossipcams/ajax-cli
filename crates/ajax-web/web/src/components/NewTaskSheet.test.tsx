import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import NewTaskSheet from "./NewTaskSheet";
import newTaskSheetSource from "./NewTaskSheet.tsx?raw";
import * as api from "../api";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readFileSync(join(here, "../styles.css"), "utf8");

const repos = [{ name: "web" }, { name: "api" }];

afterEach(() => vi.restoreAllMocks());

describe("NewTaskSheet", () => {
  it("exposes data-testid new-task-sheet", () => {
    const { getByTestId } = render(<NewTaskSheet repos={repos} />);
    expect(getByTestId("new-task-sheet")).toHaveAttribute("id", "new-task-sheet");
  });

  it("moves focus onto the dialog when opened", () => {
    const { getByTestId } = render(<NewTaskSheet repos={repos} />);
    expect(document.activeElement).toBe(getByTestId("new-task-sheet"));
  });

  it("hints the go key on the title input", () => {
    const { container } = render(<NewTaskSheet repos={repos} />);
    expect(container.querySelector("#new-task-title-input")).toHaveAttribute("enterkeyhint", "go");
  });

  it("scrolls the sheet card internally when content exceeds the band", () => {
    expect(newTaskSheetSource).toMatch(/FullscreenLayer/);
    expect(newTaskSheetSource).not.toMatch(/--app-height|--app-top/);
    expect(stylesSource).toMatch(/\.sheet-card\s*\{[^}]*overflow-y:\s*auto/);
    expect(stylesSource).toMatch(/\.sheet-card\s*\{[^}]*max-height:\s*calc\(100% - 40px\)/);
    const layerCss = stylesSource.match(/\.fullscreen-layer\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(layerCss).toMatch(/position:\s*fixed/);
    expect(layerCss).toMatch(/top:\s*var\(--app-top,\s*var\(--app-band-top,\s*0px\)\)/);
    expect(layerCss).toMatch(
      /height:\s*var\(--app-height,\s*var\(--app-band-height,\s*100dvh\)\)/,
    );
    expect(layerCss).not.toMatch(/bottom:\s*max/);
  });

  it("offers every supported agent including opencode", () => {
    const { container } = render(<NewTaskSheet repos={repos} />);
    const options = [...container.querySelectorAll<HTMLButtonElement>(".agent-option")];
    expect(options.map((option) => option.textContent?.trim())).toEqual([
      "Codex",
      "Claude",
      "Cursor",
      "OpenCode",
    ]);
    expect(newTaskSheetSource).toMatch(/role="radiogroup"/);
    expect(newTaskSheetSource).not.toMatch(/<select id="new-task-agent"/);
  });

  it("submits the selected opencode agent", async () => {
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const { container, getByRole } = render(<NewTaskSheet repos={repos} />);
    await fireEvent.input(container.querySelector("#new-task-title-input")!, {
      target: { value: "Fix login" },
    });
    await fireEvent.click(getByRole("radio", { name: "OpenCode" }));
    await fireEvent.submit(container.querySelector("form")!);
    expect(spy.mock.calls[0][0].agent).toBe("opencode");
  });

  it("preselects the matching repo for the selected project", () => {
    const { container } = render(<NewTaskSheet repos={repos} selectedProject="api" />);
    const select = container.querySelector<HTMLSelectElement>("#new-task-repo")!;
    expect(select.value).toBe("api");
  });

  it("dismisses when the grabber is dragged down past the threshold", () => {
    const onClose = vi.fn();
    const { container } = render(<NewTaskSheet repos={repos} onClose={onClose} />);
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
    const { container, getByText } = render(<NewTaskSheet repos={repos} />);
    await fireEvent.submit(container.querySelector("form")!);
    expect(getByText("Add a title")).toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
  });

  it("opens the new task route on successful start", async () => {
    vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const onOpenTask = vi.fn();
    const onClose = vi.fn();
    const { container } = render(
      <NewTaskSheet repos={repos} onOpenTask={onOpenTask} onClose={onClose} />,
    );
    await fireEvent.input(container.querySelector("#new-task-title-input")!, {
      target: { value: "Fix Login" },
    });
    await fireEvent.submit(container.querySelector("form")!);
    expect(onOpenTask).toHaveBeenCalledWith("web/fix-login");
    expect(onClose).toHaveBeenCalledOnce();
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
    const { container } = render(
      <NewTaskSheet repos={repos} onCockpit={onCockpit} onClose={onClose} />,
    );
    await fireEvent.input(container.querySelector("#new-task-title-input")!, {
      target: { value: "Fix login" },
    });
    await fireEvent.submit(container.querySelector("form")!);
    expect(spy).toHaveBeenCalledOnce();
    const arg = spy.mock.calls[0][0];
    expect(arg.title).toBe("Fix login");
    expect(arg.request_id).toEqual(expect.any(String));
    expect(arg.request_id.length).toBeGreaterThan(0);
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
    const { container, findByText } = render(<NewTaskSheet repos={repos} onClose={onClose} />);
    await fireEvent.input(container.querySelector("#new-task-title-input")!, {
      target: { value: "x" },
    });
    await fireEvent.submit(container.querySelector("form")!);
    expect(await findByText("Repo busy")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("NewTaskSheet remembered defaults", () => {
  afterEach(() => localStorage.clear());

  it("restores the last-used agent and repo", () => {
    localStorage.setItem("ajax.newTask.agent", "cursor");
    localStorage.setItem("ajax.newTask.repo", "api");
    const { container, getByRole } = render(<NewTaskSheet repos={repos} />);
    expect(getByRole("radio", { name: "Cursor" })).toHaveAttribute("aria-checked", "true");
    expect(container.querySelector<HTMLSelectElement>("#new-task-repo")!.value).toBe("api");
  });

  it("prefers the selected project over the remembered repo", () => {
    localStorage.setItem("ajax.newTask.repo", "web");
    const { container } = render(<NewTaskSheet repos={repos} selectedProject="api" />);
    expect(container.querySelector<HTMLSelectElement>("#new-task-repo")!.value).toBe("api");
  });

  it("ignores a remembered repo that is no longer configured", () => {
    localStorage.setItem("ajax.newTask.repo", "gone");
    localStorage.setItem("ajax.newTask.agent", "not-an-agent");
    const { container, getByRole } = render(<NewTaskSheet repos={repos} />);
    expect(container.querySelector<HTMLSelectElement>("#new-task-repo")!.value).toBe("web");
    expect(getByRole("radio", { name: "Codex" })).toHaveAttribute("aria-checked", "true");
  });

  it("remembers the agent and repo after a successful start", async () => {
    vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const { container, getByRole } = render(<NewTaskSheet repos={repos} />);
    await fireEvent.input(container.querySelector("#new-task-title-input")!, {
      target: { value: "Fix login" },
    });
    await fireEvent.click(getByRole("radio", { name: "OpenCode" }));
    await fireEvent.submit(container.querySelector("form")!);
    expect(localStorage.getItem("ajax.newTask.agent")).toBe("opencode");
    expect(localStorage.getItem("ajax.newTask.repo")).toBe("web");
  });
});
