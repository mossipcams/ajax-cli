import type { ReactNode } from "react";
import { useViewportBand } from "../react/useViewportBand";

interface Props {
  children?: ReactNode;
}

/** Sole consumer of initViewport's --app-top / --app-height on <html>. */
export default function AppViewport({ children }: Props) {
  useViewportBand();
  return (
    <div data-testid="app-viewport" className="app-viewport">
      {children}
    </div>
  );
}
