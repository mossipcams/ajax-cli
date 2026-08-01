import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import TerminalComposer from "./TerminalComposer";

function renderComposer(overrides: Partial<React.ComponentProps<typeof TerminalComposer>> = {}) {
  return render(
    <TerminalComposer
      value="existing prompt"
      partialText="draft words"
      state="listening"
      onChange={vi.fn()}
      onInsert={vi.fn()}
      {...overrides}
    />,
  );
}

describe("TerminalComposer", () => {
  it("preserves editable text while showing partial speech separately", () => {
    renderComposer();

    const composer = screen.getByRole("textbox", { name: "Terminal composer" });
    expect(composer).toHaveValue("existing prompt");
    expect(screen.getByTestId("terminal-composer-partial")).toHaveTextContent("draft words");
    expect(composer).not.toHaveValue("draft words");
  });

  it("only inserts text after the explicit Insert action", () => {
    const onInsert = vi.fn();
    renderComposer({ onInsert });

    expect(onInsert).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Insert transcript" }));
    expect(onInsert).toHaveBeenCalledWith("existing prompt");
  });

  it("shows pause countdown and prevents insert while finalizing", () => {
    const view = renderComposer({ state: "pause_pending", pauseCountdownSeconds: 7 });

    expect(screen.getByRole("status")).toHaveTextContent("Pausing in 7…");
    expect(screen.getByRole("status")).toHaveTextContent("Speak to continue");
    expect(screen.getByRole("button", { name: "Insert transcript" })).toBeEnabled();

    view.unmount();
    renderComposer({ state: "finalizing", pauseCountdownSeconds: undefined });
    expect(screen.getByRole("button", { name: "Insert transcript" })).toBeDisabled();
  });

  it("keeps the visible composer label and exposes a disabled error state", () => {
    renderComposer({ state: "error", errorMessage: "Microphone permission denied." });

    expect(screen.getByRole("textbox", { name: "Terminal composer" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("Microphone permission denied.");
  });

  it.each([
    ["connecting", "Connecting…"],
    ["listening", "Listening"],
    ["finalizing", "Finalizing…"],
  ] as const)("announces the %s speech state", (state, label) => {
    renderComposer({ state });

    expect(screen.getByRole("status")).toHaveTextContent(label);
  });
});
