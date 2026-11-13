import { Activity, type CSSProperties, type ReactNode } from "react";
import type { Route } from "@/shared/lib/routes";
import { usePageCrossSlide } from "@/shared/hooks/useSwipePageTransition";

function workspaceSurfaceVisible(
  surface: "task" | "session" | "diff",
  activeRoute: Route,
  leaving: Route | null,
): boolean {
  const routeMatches = (candidate: Route) => {
    if (surface === "task") return candidate.kind === "task";
    if (surface === "session") return candidate.kind === "session" && Boolean(candidate.handle);
    return candidate.kind === "diff";
  };
  return routeMatches(activeRoute) || (leaving ? routeMatches(leaving) : false);
}

function workspaceSurfaceKind(surface: "task" | "session" | "diff"): Route["kind"] {
  if (surface === "session") return "session";
  if (surface === "diff") return "diff";
  return "task";
}

export default function WorkspaceStack({
  handle,
  route,
  surfaces,
}: {
  handle: string;
  route: Route;
  surfaces: { task: ReactNode; session: ReactNode | null; diff: ReactNode };
}) {
  const crossSlide = usePageCrossSlide();
  const crossSlideActive = crossSlide?.active ?? false;
  const leavingRoute = crossSlide?.leavingRoute ?? null;

  function workspacePaneStyle(surface: "task" | "session" | "diff"): CSSProperties | undefined {
    if (!crossSlideActive || !crossSlide || !leavingRoute) return undefined;
    if (workspaceSurfaceKind(surface) === leavingRoute.kind) {
      return crossSlide.paneStyle("leaving");
    }
    if (workspaceSurfaceKind(surface) === route.kind) {
      return crossSlide.paneStyle("entering");
    }
    return undefined;
  }

  const surfaceShell = (surface: "task" | "session" | "diff", content: ReactNode) => {
    const visible = workspaceSurfaceVisible(surface, route, leavingRoute);
    const leavingSurface = leavingRoute?.kind === workspaceSurfaceKind(surface);
    const mountContent = visible || (crossSlideActive && leavingSurface);
    const motionStyle = workspacePaneStyle(surface);
    const paneAnimating = motionStyle !== undefined;
    const isLeaving = leavingSurface;
    const isEntering = crossSlideActive && route.kind === workspaceSurfaceKind(surface);
    return (
      <Activity key={`activity-${surface}-${handle}`} mode={visible ? "visible" : "hidden"}>
        {mountContent ? (
          <div
            className={[
              "page-cross-slide-pane",
              paneAnimating
                ? isLeaving
                  ? "page-cross-slide-leaving"
                  : "page-cross-slide-entering"
                : "page-cross-slide-pane-idle",
            ].join(" ")}
            style={motionStyle}
            onTransitionEnd={isEntering ? crossSlide?.onEnteringTransitionEnd : undefined}
            aria-hidden={isLeaving ? true : undefined}
          >
            {content}
          </div>
        ) : null}
      </Activity>
    );
  };

  return (
    <div className={`page-cross-slide-host${crossSlideActive ? "" : " page-cross-slide-host-idle"}`}>
      {surfaceShell("task", surfaces.task)}
      {surfaces.session !== null ? surfaceShell("session", surfaces.session) : null}
      {surfaceShell("diff", surfaces.diff)}
    </div>
  );
}
