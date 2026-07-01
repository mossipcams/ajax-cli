import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import TerminalSnapshotView from "./TerminalSnapshotView.svelte";

const openTaskTerminalSocket = vi.fn();
const fetchTaskSnapshot = vi.fn();
const sendTaskKeys = vi.fn();

vi.mock("../api", () => ({
  openTaskTerminalSocket: (handle: string) => openTaskTerminalSocket(handle),
  fetchTaskSnapshot: (handle: string, since?: string) => fetchTaskSnapshot(handle, since),
  sendTaskKeys: (handle: string, text: string, submit: boolean) =>
    sendTaskKeys(handle, text, submit),
}));

beforeEach(() => {
  openTaskTerminalSocket.mockReset();
  fetchTaskSnapshot
    .mockReset()
    .mockResolvedValue({
      sequence_changed: true,
      lines: ["ready"],
      truncated: false,
      sequence: "seq-1",
      summary: null,
    });
  sendTaskKeys.mockReset().mockResolvedValue({ ok: true });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("TerminalSnapshotView", () => {
  it("polls the snapshot endpoint and renders pane lines without any socket", async () => {
    render(TerminalSnapshotView, { props: { handle: "web/fix-login" } });

    await waitFor(() => expect(fetchTaskSnapshot).toHaveBeenCalledWith("web/fix-login", undefined));
    await waitFor(() =>
      expect(document.querySelector(".terminal-snapshot-lines pre")?.textContent).toContain("ready"),
    );
    expect(openTaskTerminalSocket).not.toHaveBeenCalled();
  });

  it("echoes the previous sequence back as `since` on the next poll", async () => {
    vi.useFakeTimers();
    render(TerminalSnapshotView, { props: { handle: "web/fix-login" } });

    // Flush the initial refresh (microtask), then advance to the next poll tick.
    await vi.advanceTimersByTimeAsync(0);
    expect(fetchTaskSnapshot).toHaveBeenNthCalledWith(1, "web/fix-login", undefined);

    await vi.advanceTimersByTimeAsync(1500);
    expect(fetchTaskSnapshot).toHaveBeenNthCalledWith(2, "web/fix-login", "seq-1");
    vi.useRealTimers();
  });

  it("submits composer text via sendTaskKeys and clears the field, no socket", async () => {
    const { getByRole } = render(TerminalSnapshotView, { props: { handle: "web/fix-login" } });

    const input = getByRole("textbox", { name: "Terminal command" }) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "approve" } });
    getByRole("button", { name: "Send" }).click();

    await waitFor(() => expect(sendTaskKeys).toHaveBeenCalledWith("web/fix-login", "approve", true));
    expect(openTaskTerminalSocket).not.toHaveBeenCalled();
    await waitFor(() => expect(input.value).toBe(""));
  });

  it("submits on Enter and keeps the field on failure with an error", async () => {
    sendTaskKeys.mockResolvedValueOnce({ ok: false, error: "tmux session missing" });
    const { getByRole, findByTestId } = render(TerminalSnapshotView, {
      props: { handle: "web/fix-login" },
    });

    const input = getByRole("textbox", { name: "Terminal command" }) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "run tests" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(sendTaskKeys).toHaveBeenCalledWith("web/fix-login", "run tests", true));
    expect(await findByTestId("composer-error")).toHaveTextContent("tmux session missing");
    expect(input.value).toBe("run tests");
  });

  it("does not replace lines when the pane is unchanged", async () => {
    vi.useFakeTimers();
    render(TerminalSnapshotView, { props: { handle: "web/fix-login" } });
    await vi.advanceTimersByTimeAsync(0);

    // Next poll reports no change: the rendered lines must be preserved.
    fetchTaskSnapshot.mockResolvedValueOnce({
      sequence_changed: false,
      lines: [],
      truncated: false,
      sequence: "seq-1",
      summary: null,
    });
    await vi.advanceTimersByTimeAsync(1500);
    await tick();

    expect(document.querySelector(".terminal-snapshot-lines pre")?.textContent).toContain("ready");
    vi.useRealTimers();
  });
});
