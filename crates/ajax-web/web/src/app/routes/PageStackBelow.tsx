import type { ReactNode } from "react";
import type { Route } from "@/shared/lib/routes";
import { readRouteScrollOffset } from "../RouteScroll";

export default function PageStackBelow({
  route,
  chrome,
  nav,
  children,
}: {
  route: Route | null;
  chrome?: ReactNode;
  nav?: ReactNode;
  children: ReactNode;
}) {
  if (!route) return null;

  return (
    <div className="page-stack-below" data-testid="page-stack-below" inert aria-hidden="true">
      {chrome}
      <div className="page-stack-below-content">
        <div
          className="page-stack-below-scroll"
          style={{ transform: `translate3d(0, ${-readRouteScrollOffset(route)}px, 0)` }}
        >
          {children}
        </div>
      </div>
      {nav}
    </div>
  );
}
