import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { render, screen, fireEvent } from "@testing-library/react";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import {
  FloatingContextMenu,
  floatingMenuShiftPadding,
  readFloatingMenuShiftPadding,
} from "./FloatingContextMenu";

const webSrcRoot = join(dirname(fileURLToPath(import.meta.url)), "../..");
const stylesSource = readOrderedStylesSource(webSrcRoot);

const menuSource = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), "FloatingContextMenu.tsx"),
  "utf8",
);

describe("FloatingContextMenu", () => {
  it("uses fixed positioning so clientX/Y anchors track the visual viewport", () => {
    expect(menuSource).toMatch(/strategy:\s*["']fixed["']/);
  });

  it("stacks above the expanded terminal panel", () => {
    // Expanded terminal uses z-index 45; without this the portaled Open/Copy
    // menu paints under the terminal and taps never reach the buttons (#708
    // restore dropped the CSS while keeping the TSX).
    const rule = stylesSource.match(
      /\.floating-context-menu\s*\{[^}]*\}/,
    )?.[0];
    expect(rule).toBeDefined();
    expect(rule).toMatch(/z-index:\s*50/);
  });

  it("renders items when open with an anchor point", () => {
    const onOpen = vi.fn();
    const onClose = vi.fn();

    render(
      <FloatingContextMenu
        open
        anchor={{ x: 40, y: 60 }}
        items={[
          { id: "open", label: "Open", onSelect: onOpen },
          { id: "copy", label: "Copy", onSelect: vi.fn() },
        ]}
        onClose={onClose}
        ariaLabel="Link actions"
      />,
    );

    expect(screen.getByRole("menuitem", { name: "Open" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Copy" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("menuitem", { name: "Open" }));
    expect(onOpen).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("fires onSelect only once under same-turn double click", () => {
    const onOpen = vi.fn();
    render(
      <FloatingContextMenu
        open
        anchor={{ x: 40, y: 60 }}
        items={[{ id: "open", label: "Open", onSelect: onOpen }]}
        onClose={vi.fn()}
      />,
    );
    const open = screen.getByRole("menuitem", { name: "Open" });
    open.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    open.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it("calls onClose on Escape", () => {
    const onClose = vi.fn();

    render(
      <FloatingContextMenu
        open
        anchor={{ x: 10, y: 10 }}
        items={[{ id: "open", label: "Open", onSelect: vi.fn() }]}
        onClose={onClose}
      />,
    );

    fireEvent.keyDown(document, { key: "Escape" });

    expect(onClose).toHaveBeenCalled();
  });

  it("calls onClose on outside press", () => {
    const onClose = vi.fn();

    render(
      <FloatingContextMenu
        open
        anchor={{ x: 10, y: 10 }}
        items={[{ id: "open", label: "Open", onSelect: vi.fn() }]}
        onClose={onClose}
      />,
    );

    fireEvent.pointerDown(document.body);

    expect(onClose).toHaveBeenCalled();
  });

  describe("scroll dismiss grace period", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("does not call onClose on scroll during the first ~800ms after open", () => {
      const onClose = vi.fn();
      const scroller = document.createElement("div");
      document.body.appendChild(scroller);

      render(
        <FloatingContextMenu
          open
          anchor={{ x: 10, y: 10 }}
          items={[{ id: "open", label: "Open", onSelect: vi.fn() }]}
          onClose={onClose}
        />,
      );

      scroller.dispatchEvent(new Event("scroll", { bubbles: true }));
      vi.advanceTimersByTime(700);
      scroller.dispatchEvent(new Event("scroll", { bubbles: true }));

      expect(onClose).not.toHaveBeenCalled();
      scroller.remove();
    });

    it("calls onClose on scroll after the grace window", () => {
      const onClose = vi.fn();
      const scroller = document.createElement("div");
      document.body.appendChild(scroller);

      render(
        <FloatingContextMenu
          open
          anchor={{ x: 10, y: 10 }}
          items={[{ id: "open", label: "Open", onSelect: vi.fn() }]}
          onClose={onClose}
        />,
      );

      vi.advanceTimersByTime(850);
      scroller.dispatchEvent(new Event("scroll", { bubbles: true }));

      expect(onClose).toHaveBeenCalled();
      scroller.remove();
    });
  });

  it("exposes safe-area aware shift padding", () => {
    const padding = readFloatingMenuShiftPadding();
    expect(padding).toEqual({
      top: expect.any(Number),
      right: expect.any(Number),
      bottom: expect.any(Number),
      left: expect.any(Number),
    });
    expect(floatingMenuShiftPadding).toEqual(padding);

    render(
      <FloatingContextMenu
        open
        anchor={{ x: 10, y: 10 }}
        items={[{ id: "open", label: "Open", onSelect: vi.fn() }]}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByRole("menu")).toHaveClass("floating-context-menu");
  });
});
