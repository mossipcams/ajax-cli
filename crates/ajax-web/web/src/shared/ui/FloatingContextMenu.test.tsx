import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import {
  FloatingContextMenu,
  floatingMenuShiftPadding,
  readFloatingMenuShiftPadding,
} from "./FloatingContextMenu";

describe("FloatingContextMenu", () => {
  it("renders items when open with an anchor point", () => {
    const onOpen = vi.fn();
    const onCopy = vi.fn();

    render(
      <FloatingContextMenu
        open
        anchor={{ x: 40, y: 60 }}
        items={[
          { id: "open", label: "Open", onSelect: onOpen },
          { id: "copy", label: "Copy", onSelect: onCopy },
        ]}
        onClose={vi.fn()}
        ariaLabel="Link actions"
      />,
    );

    fireEvent.click(screen.getByRole("menuitem", { name: "Open" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy" }));

    expect(onOpen).toHaveBeenCalledOnce();
    expect(onCopy).toHaveBeenCalledOnce();
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

    it("does not call onClose on scroll during the first ~400ms after open", () => {
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
      vi.advanceTimersByTime(300);
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

      vi.advanceTimersByTime(450);
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
