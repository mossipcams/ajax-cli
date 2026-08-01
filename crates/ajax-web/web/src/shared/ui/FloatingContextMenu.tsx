import { useEffect, useMemo } from "react";
import {
  autoUpdate,
  flip,
  FloatingPortal,
  offset,
  shift,
  useDismiss,
  useFloating,
  useInteractions,
  useRole,
  type VirtualElement,
} from "@floating-ui/react";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/utils";

export type FloatingContextMenuItem = {
  id: string;
  label: string;
  onSelect: () => void;
};

export type FloatingContextMenuAnchor = { x: number; y: number } | DOMRect;

export type FloatingContextMenuProps = {
  open: boolean;
  anchor: FloatingContextMenuAnchor | null;
  items: FloatingContextMenuItem[];
  onClose: () => void;
  ariaLabel?: string;
};

function readSafeAreaInset(side: "top" | "right" | "bottom" | "left"): number {
  if (typeof window === "undefined" || typeof document === "undefined") return 0;
  const probe = document.createElement("div");
  probe.style.position = "fixed";
  probe.style.visibility = "hidden";
  probe.style.pointerEvents = "none";
  const property =
    side === "top"
      ? "paddingTop"
      : side === "right"
        ? "paddingRight"
        : side === "bottom"
          ? "paddingBottom"
          : "paddingLeft";
  probe.style[property] = `env(safe-area-inset-${side})`;
  document.body.appendChild(probe);
  const value = Number.parseFloat(getComputedStyle(probe)[property] || "0");
  probe.remove();
  return Number.isFinite(value) ? value : 0;
}

/** Viewport padding for flip/shift so the menu clears iOS Safari safe areas. */
export function readFloatingMenuShiftPadding(): {
  top: number;
  right: number;
  bottom: number;
  left: number;
} {
  return {
    top: readSafeAreaInset("top"),
    right: readSafeAreaInset("right"),
    bottom: readSafeAreaInset("bottom"),
    left: readSafeAreaInset("left"),
  };
}

/** @deprecated Prefer `readFloatingMenuShiftPadding()` so insets refresh per open. */
export const floatingMenuShiftPadding = readFloatingMenuShiftPadding();

function toVirtualElement(anchor: FloatingContextMenuAnchor): VirtualElement {
  if (anchor instanceof DOMRect) {
    return {
      getBoundingClientRect: () => anchor,
    };
  }
  return {
    getBoundingClientRect: () => ({
      x: anchor.x,
      y: anchor.y,
      width: 0,
      height: 0,
      top: anchor.y,
      left: anchor.x,
      right: anchor.x,
      bottom: anchor.y,
    }),
  };
}

/** Ignore scroll-dismiss briefly after open so opening tap scroll does not close the menu. */
const SCROLL_DISMISS_GRACE_MS = 800;

export function FloatingContextMenu({
  open,
  anchor,
  items,
  onClose,
  ariaLabel = "Context menu",
}: FloatingContextMenuProps) {
  const virtualReference = useMemo(
    () => (anchor ? toVirtualElement(anchor) : null),
    [anchor],
  );

  const shiftPadding = useMemo(
    () => (open ? readFloatingMenuShiftPadding() : floatingMenuShiftPadding),
    [open],
  );

  const { refs, floatingStyles, context } = useFloating({
    open,
    onOpenChange: (nextOpen) => {
      if (!nextOpen) onClose();
    },
    placement: "bottom-start",
    strategy: "fixed",
    middleware: [offset(8), flip(), shift({ padding: shiftPadding })],
    whileElementsMounted: autoUpdate,
  });

  useEffect(() => {
    if (!virtualReference) {
      refs.setPositionReference(null);
      return;
    }
    refs.setPositionReference(virtualReference);
  }, [refs, virtualReference]);

  const dismiss = useDismiss(context, {
    outsidePress: true,
    escapeKey: true,
    ancestorScroll: false,
  });
  const role = useRole(context, { role: "menu" });
  const { getFloatingProps } = useInteractions([dismiss, role]);

  useEffect(() => {
    if (!open) return;
    const openedAt = performance.now();
    const onScroll = () => {
      if (performance.now() - openedAt < SCROLL_DISMISS_GRACE_MS) return;
      onClose();
    };
    window.addEventListener("scroll", onScroll, true);
    return () => window.removeEventListener("scroll", onScroll, true);
  }, [open, onClose]);

  if (!open || !anchor) return null;

  return (
    <FloatingPortal>
      <div
        ref={refs.setFloating}
        className={cn("floating-context-menu")}
        style={floatingStyles}
        role="menu"
        aria-label={ariaLabel}
        {...getFloatingProps()}>
        {items.map((item) => (
          <Button
            key={item.id}
            type="button"
            variant="secondary"
            className="floating-context-menu__item"
            role="menuitem"
            onClick={() => item.onSelect()}>
            {item.label}
          </Button>
        ))}
      </div>
    </FloatingPortal>
  );
}
