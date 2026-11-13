import type { ReactNode } from "react";
import type { Route } from "@/shared/lib/routes";
import { readRouteScrollOffset } from "../RouteScroll";

export default function PageStackBelow({
  route,
  children,
}: {
  route: Route | null;
  children: ReactNode;
}) {
  if (!route) return null;

  return (
    <div className="page-stack-below" data-testid="page-stack-below" inert aria-hidden="true">
      <div
        className="page-stack-below-content"
        style={{ transform: `translate3d(0, ${-readRouteScrollOffset(route)}px, 0)` }}
      >
        {children}
      </div>
    </div>
  );
}
