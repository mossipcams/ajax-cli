import { useLayoutEffect, useRef, type ReactNode } from "react";
import { useHashRoute } from "@/shared/hooks/useHashRoute";
import type { Route } from "@/shared/lib/routes";

interface Props {
  children?: ReactNode;
  restoreKey?: string | null;
}

const RESTORE_WINDOW_MS = 600;
const INTERRUPT_EVENTS = ["touchstart", "wheel", "pointerdown", "keydown"] as const;

const offsets = new Map<string, number>();

export function readRouteScrollOffset(route: Route): number {
  return offsets.get(JSON.stringify(route)) ?? 0;
}

export default function RouteScroll({ children, restoreKey }: Props) {
  const route = useHashRoute();
  const routeKey = JSON.stringify(route);
  const ref = useRef<HTMLDivElement>(null);
  const restoring = useRef(false);

  useLayoutEffect(() => () => offsets.clear(), []);

  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const target = offsets.get(routeKey) ?? 0;
    const deadline = performance.now() + RESTORE_WINDOW_MS;
    let frame = 0;
    const stop = () => {
      restoring.current = false;
      cancelAnimationFrame(frame);
      for (const type of INTERRUPT_EVENTS) node.removeEventListener(type, stop);
    };
    const apply = () => {
      if (node.scrollTop !== target) node.scrollTop = target;
      if (node.scrollTop === target || performance.now() >= deadline) {
        stop();
        return;
      }
      frame = requestAnimationFrame(apply);
    };
    restoring.current = true;
    for (const type of INTERRUPT_EVENTS) node.addEventListener(type, stop, { passive: true });
    apply();
    return stop;
  }, [routeKey, restoreKey]);

  return (
    <div
      ref={ref}
      data-testid="route-scroll"
      className="route-scroll"
      onScroll={() => {
        if (!restoring.current && ref.current) {
          offsets.set(routeKey, ref.current.scrollTop);
        }
      }}
    >
      {children}
    </div>
  );
}
