import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { StrictMode } from "react";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import NewTaskSheet from "./NewTaskSheet";
import newTaskSheetSource from "./NewTaskSheet.tsx?raw";
import * as api from "@/shared/lib/api";
import { writeOrchestrationChatEnabled } from "@/features/session/sessionMode";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readFileSync(join(here, "../../styles.css"), "utf8");

const repos = [{ name: "web" }, { name: "api" }];

const CATALOG = {
  models: [
    { id: "gpt-5.6-sol[low]", label: "GPT-5.6-Sol (low)" },
    { id: "gpt-5.6-sol[high]", label: "GPT-5.6-Sol (high)" },
  ],
  default: "gpt-5.6-sol[low]",
};

// Claude and the other bridges keep the reasoning level in its own option.
const CATALOG_WITH_REASONING = {
  models: [
    { id: "opus", label: "Opus" },
    { id: "haiku", label: "Haiku" },
  ],
  default: "opus",
  reasoning: {
    id: "effort",
    label: "Effort",
    default: "high",
    options: [
      { id: "low", label: "Low" },
      { id: "high", label: "High" },
    ],
  },
};

function stubCatalog(catalog: unknown = CATALOG) {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({ ok: true, json: async () => catalog }),
  );
}

const taskForm = () => screen.getByRole("form", { name: "New task" });

/** Step one → step two. Start only exists on the model page. */
async function goToModelStep() {
  fireEvent.submit(taskForm());
  return screen.findByTestId("new-task-model-page");
}

// The sheet remembers the last model per harness, so tests must not inherit
// each other's choices.
beforeEach(() => localStorage.clear());
afterEach(() => vi.restoreAllMocks());

describe("NewTaskSheet", () => {
  it("exposes data-testid new-task-sheet", () => {
    render(<NewTaskSheet repos={repos} />);
    expect(screen.getByTestId("new-task-sheet")).toHaveAttribute("id", "new-task-sheet");
  });

  it("moves focus onto the dialog when opened", () => {
    render(<NewTaskSheet repos={repos} />);
    expect(screen.getByTestId("new-task-sheet")).toHaveFocus();
  });

  it("hints the go key on the title input", () => {
    render(<NewTaskSheet repos={repos} />);
    expect(screen.getByLabelText("Title")).toHaveAttribute("enterkeyhint", "go");
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

  it("offers every supported agent including pi", () => {
    render(<NewTaskSheet repos={repos} />);
    expect(screen.getByRole("radio", { name: "Codex" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Claude" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Cursor" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Pi" })).toBeInTheDocument();
    expect(newTaskSheetSource).toMatch(/role="radiogroup"/);
    expect(newTaskSheetSource).not.toMatch(/<select id="new-task-agent"/);
  });

  it("submits the selected pi agent", async () => {
    stubCatalog();
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "Fix login" },
    });
    fireEvent.click(screen.getByRole("radio", { name: "Pi" }));
    await goToModelStep();
    fireEvent.submit(taskForm());
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[0][0].agent).toBe("pi");
    expect(spy.mock.calls[0][0].orchestration_chat).toBe(true);
    vi.unstubAllGlobals();
  });

  it("omits orchestration_chat when the preference is explicitly off", async () => {
    writeOrchestrationChatEnabled(false);
    stubCatalog();
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "Fix login" },
    });
    await goToModelStep();
    fireEvent.submit(taskForm());
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[0][0].orchestration_chat).toBeUndefined();
    vi.unstubAllGlobals();
  });

  it("moves to a model page listing what the chosen harness advertises", async () => {
    stubCatalog();
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    fireEvent.click(screen.getByRole("radio", { name: "Codex" }));

    expect(screen.queryByTestId("new-task-model-page")).not.toBeInTheDocument();
    await goToModelStep();

    expect(await screen.findByRole("radio", { name: /GPT-5.6-Sol \(high\)/ })).toBeInTheDocument();
    const fetchMock = vi.mocked(globalThis.fetch);
    expect(fetchMock.mock.calls[0]?.[0]).toContain("agent=codex");
    vi.unstubAllGlobals();
  });

  it("starts on the harness default and submits the picked model", async () => {
    stubCatalog();
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    fireEvent.click(screen.getByRole("radio", { name: "Codex" }));
    await goToModelStep();

    const preselected = await screen.findByRole("radio", { name: /GPT-5.6-Sol \(low\)/ });
    expect(preselected).toHaveAttribute("aria-checked", "true");

    fireEvent.click(screen.getByRole("radio", { name: /GPT-5.6-Sol \(high\)/ }));
    fireEvent.submit(taskForm());
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[0][0].model).toBe("gpt-5.6-sol[high]");
    vi.unstubAllGlobals();
  });

  it("shows the harness reasoning level and sends it with the model", async () => {
    stubCatalog(CATALOG_WITH_REASONING);
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    fireEvent.click(screen.getByRole("radio", { name: "Claude" }));
    await goToModelStep();

    // The harness's own current level is preselected, not the first option.
    const level = await screen.findByRole("radio", { name: "High" });
    expect(level).toHaveAttribute("aria-checked", "true");

    fireEvent.click(screen.getByRole("radio", { name: "Low" }));
    fireEvent.submit(taskForm());
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[0][0].model).toBe("opus|effort=low");
    vi.unstubAllGlobals();
  });

  it("keeps the reasoning level when the model changes", async () => {
    stubCatalog(CATALOG_WITH_REASONING);
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    fireEvent.click(screen.getByRole("radio", { name: "Claude" }));
    await goToModelStep();

    fireEvent.click(await screen.findByRole("radio", { name: "Low" }));
    fireEvent.click(screen.getByRole("radio", { name: "Haiku" }));
    fireEvent.submit(taskForm());
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[0][0].model).toBe("haiku|effort=low");
    vi.unstubAllGlobals();
  });

  it("offers no reasoning row for a harness that has none", async () => {
    stubCatalog();
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    await goToModelStep();
    await screen.findByRole("radio", { name: /GPT-5.6-Sol \(low\)/ });
    expect(screen.queryByTestId("model-reasoning")).not.toBeInTheDocument();
    vi.unstubAllGlobals();
  });

  // Found in dev: an nvm switch left the bridges off the server's PATH and the
  // page said the harness "lists no models", hiding a fixable install problem.
  it("shows why a harness could not be read instead of an empty list", async () => {
    stubCatalog({
      models: [],
      default: "",
      error: "codex-acp is not installed — npm install -g @agentclientprotocol/codex-acp",
    });
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    fireEvent.click(screen.getByRole("radio", { name: "Codex" }));
    await goToModelStep();

    expect(await screen.findByTestId("model-catalog-error")).toHaveTextContent(
      "codex-acp is not installed",
    );
    vi.unstubAllGlobals();
  });

  it("Back returns to the task page without starting anything", async () => {
    stubCatalog();
    const spy = vi.spyOn(api, "startTask");
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    await goToModelStep();

    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByLabelText("Title")).toHaveValue("Fix login");
    expect(screen.queryByTestId("new-task-model-page")).not.toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("sends no model when the harness advertises none", async () => {
    stubCatalog({ models: [], default: "" });
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    fireEvent.click(screen.getByRole("radio", { name: "Claude" }));
    await goToModelStep();
    fireEvent.submit(taskForm());
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[0][0].model).toBeUndefined();
    vi.unstubAllGlobals();
  });

  it("preselects the matching repo for the selected project", () => {
    render(<NewTaskSheet repos={repos} selectedProject="api" />);
    expect(screen.getByLabelText("Repository")).toHaveValue("api");
  });

  it("dismisses when the grabber is dragged down past the threshold", () => {
    const onClose = vi.fn();
    render(<NewTaskSheet repos={repos} onClose={onClose} />);
    const grab = screen.getByTestId("sheet-grab");

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

  it("rejects an empty title locally without calling the API", () => {
    const spy = vi.spyOn(api, "startTask");
    render(<NewTaskSheet repos={repos} />);
    fireEvent.submit(screen.getByRole("form", { name: "New task" }));
    expect(screen.getByText("Add a title")).toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
  });

  it("adopts the first repository when repository data arrives after opening", async () => {
    stubCatalog();
    const spy = vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const { rerender } = render(<NewTaskSheet repos={[]} />);

    rerender(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Late repos" } });
    await goToModelStep();
    fireEvent.submit(taskForm());

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy.mock.calls[0][0].repo).toBe("web");
    vi.unstubAllGlobals();
  });

  it("#855 does not navigate when Start succeeds after the sheet unmounts", async () => {
    stubCatalog();
    const cockpit = {
      backend: { authority: "host-native", control_enabled: true },
      repos: { repos: [] },
      cards: [],
      inbox: { items: [] },
    };
    let resolveStart!: (value: Awaited<ReturnType<typeof api.startTask>>) => void;
    const pending = new Promise<Awaited<ReturnType<typeof api.startTask>>>((resolve) => {
      resolveStart = resolve;
    });
    vi.spyOn(api, "startTask").mockReturnValue(pending as never);
    const onCockpit = vi.fn();
    const onOpenTask = vi.fn();
    const onClose = vi.fn();
    const { unmount } = render(
      <NewTaskSheet
        repos={repos}
        onCockpit={onCockpit}
        onOpenTask={onOpenTask}
        onClose={onClose}
      />,
    );
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix login" } });
    await goToModelStep();
    fireEvent.submit(taskForm());
    unmount();
    resolveStart({ ok: true, response: { cockpit } });
    await waitFor(() => expect(onCockpit).toHaveBeenCalledWith(cockpit));
    expect(onOpenTask).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("#855 still opens the task after StrictMode remounts the sheet", async () => {
    vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const onOpenTask = vi.fn();
    const onClose = vi.fn();
    render(
      <StrictMode>
        <NewTaskSheet repos={repos} onOpenTask={onOpenTask} onClose={onClose} />
      </StrictMode>,
    );
    fireEvent.input(screen.getByLabelText("Title"), { target: { value: "Fix Login" } });
    await goToModelStep();
    fireEvent.submit(taskForm());
    await waitFor(() => expect(onOpenTask).toHaveBeenCalledWith("web/fix-login"));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("opens the new task route on successful start", async () => {
    vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    const onOpenTask = vi.fn();
    const onClose = vi.fn();
    render(<NewTaskSheet repos={repos} onOpenTask={onOpenTask} onClose={onClose} />);
    fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "Fix Login" },
    });
    await goToModelStep();
    fireEvent.submit(taskForm());
    await waitFor(() => expect(onOpenTask).toHaveBeenCalledWith("web/fix-login"));
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
    render(<NewTaskSheet repos={repos} onCockpit={onCockpit} onClose={onClose} />);
    fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "Fix login" },
    });
    await goToModelStep();
    fireEvent.submit(taskForm());
    await waitFor(() => expect(spy).toHaveBeenCalledOnce());
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
    render(<NewTaskSheet repos={repos} onClose={onClose} />);
    fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "x" },
    });
    await goToModelStep();
    fireEvent.submit(taskForm());
    expect(await screen.findByText("Repo busy")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("renders a network error and keeps the sheet open", async () => {
    vi.spyOn(api, "startTask").mockRejectedValue(new Error("network"));
    const onClose = vi.fn();
    render(<NewTaskSheet repos={repos} onClose={onClose} />);
    fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "x" },
    });
    await goToModelStep();
    fireEvent.submit(taskForm());
    expect(await screen.findByText("Action failed — network error")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("closes when Escape is pressed on the dialog", () => {
    const onClose = vi.fn();
    render(<NewTaskSheet repos={repos} onClose={onClose} />);
    fireEvent.keyDown(screen.getByTestId("new-task-sheet"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("closes on a backdrop click but not on a click inside the card", () => {
    const onClose = vi.fn();
    render(<NewTaskSheet repos={repos} onClose={onClose} />);
    fireEvent.click(screen.getByRole("heading", { name: "New task" }));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("new-task-sheet"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("labels the dialog with a title that actually exists", () => {
    render(<NewTaskSheet repos={repos} />);
    // Radix wires aria-labelledby to the title; accessible name must resolve.
    expect(screen.getByRole("dialog", { name: "New task" })).toHaveAttribute("aria-modal", "true");
  });

  it("keeps the agent picker a single tab stop", () => {
    localStorage.clear(); // an earlier test in this file persists a remembered agent
    render(<NewTaskSheet repos={repos} />);
    expect(screen.getByRole("radio", { name: "Codex" })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("radio", { name: "Claude" })).toHaveAttribute("tabindex", "-1");
    expect(screen.getByRole("radio", { name: "Cursor" })).toHaveAttribute("tabindex", "-1");
    expect(screen.getByRole("radio", { name: "Pi" })).toHaveAttribute("tabindex", "-1");
  });

  it("moves selection and focus with arrow keys", () => {
    localStorage.clear(); // start from Codex regardless of test order
    render(<NewTaskSheet repos={repos} />);
    fireEvent.keyDown(screen.getByRole("radio", { name: "Codex" }), { key: "ArrowRight" });
    expect(screen.getByRole("radio", { name: "Claude" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "Claude" })).toHaveFocus();
    // Wraps backwards off the first option.
    fireEvent.keyDown(screen.getByRole("radio", { name: "Claude" }), { key: "ArrowLeft" });
    fireEvent.keyDown(screen.getByRole("radio", { name: "Codex" }), { key: "ArrowLeft" });
    expect(screen.getByRole("radio", { name: "Pi" })).toHaveAttribute("aria-checked", "true");
  });

  it("restores focus to the opener when the sheet unmounts", () => {
    const opener = document.createElement("button");
    opener.textContent = "Open new task";
    document.body.appendChild(opener);
    opener.focus();
    expect(opener).toHaveFocus();

    const { unmount } = render(<NewTaskSheet repos={repos} />);
    unmount();

    expect(opener).toHaveFocus();
    opener.remove();
  });
});

describe("NewTaskSheet remembered defaults", () => {
  afterEach(() => localStorage.clear());

  it("restores the last-used agent and repo", () => {
    localStorage.setItem("ajax.newTask.agent", "cursor");
    localStorage.setItem("ajax.newTask.repo", "api");
    render(<NewTaskSheet repos={repos} />);
    expect(screen.getByRole("radio", { name: "Cursor" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByLabelText("Repository")).toHaveValue("api");
  });

  it("prefers the selected project over the remembered repo", () => {
    localStorage.setItem("ajax.newTask.repo", "web");
    render(<NewTaskSheet repos={repos} selectedProject="api" />);
    expect(screen.getByLabelText("Repository")).toHaveValue("api");
  });

  it("ignores a remembered repo that is no longer configured", () => {
    localStorage.setItem("ajax.newTask.repo", "gone");
    localStorage.setItem("ajax.newTask.agent", "not-an-agent");
    render(<NewTaskSheet repos={repos} />);
    expect(screen.getByLabelText("Repository")).toHaveValue("web");
    expect(screen.getByRole("radio", { name: "Codex" })).toHaveAttribute("aria-checked", "true");
  });

  it("remembers the agent and repo after a successful start", async () => {
    vi.spyOn(api, "startTask").mockResolvedValue({ ok: true, response: {} });
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "Fix login" },
    });
    fireEvent.click(screen.getByRole("radio", { name: "Pi" }));
    await goToModelStep();
    fireEvent.submit(taskForm());
    await waitFor(() => expect(localStorage.getItem("ajax.newTask.agent")).toBe("pi"));
    expect(localStorage.getItem("ajax.newTask.repo")).toBe("web");
  });

  it("posts startTask only once under rapid double submit", async () => {
    let resolveFirst!: (value: { ok: boolean; response: object }) => void;
    const pending = new Promise<{ ok: boolean; response: object }>((resolve) => {
      resolveFirst = resolve;
    });
    const spy = vi.spyOn(api, "startTask").mockReturnValue(pending as never);
    render(<NewTaskSheet repos={repos} />);
    fireEvent.input(screen.getByLabelText("Title"), {
      target: { value: "Chaos duplicate" },
    });
    const form = screen.getByRole("form", { name: "New task" });
    fireEvent.submit(form);
    fireEvent.submit(form);
    expect(spy).toHaveBeenCalledTimes(1);
    resolveFirst({ ok: true, response: {} });
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
  });
});
