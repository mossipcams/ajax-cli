import { useLayoutEffect, useRef, type ReactNode } from "react";
import { useHashRoute } from "@/shared/hooks/useHashRoute";

interface Props {
  children?: ReactNode;
}

const RESTORE_WINDOW_MS = 600;
const INTERRUPT_EVENTS = ["touchstart", "wheel", "pointerdown", "keydown"] as const;

export default function RouteScroll({ children }: Props) {
  const route = useHashRoute();
  const routeKey = JSON.stringify(route);
  const ref = useRef<HTMLDivElement>(null);
  const offsets = useRef(new Map<string, number>());
  const restoring = useRef(false);

  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const target = offsets.current.get(routeKey) ?? 0;
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
  }, [routeKey]);

  return (
    <div
      ref={ref}
      data-testid="route-scroll"
      className="route-scroll"
      onScroll={() => {
        if (!restoring.current && ref.current) {
          offsets.current.set(routeKey, ref.current.scrollTop);
        }
      }}
    >
      {children}
    </div>
  );
}
