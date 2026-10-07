import type { ReactNode } from "react";
import { useViewportBand } from "@/shared/hooks/useViewportBand";

interface Props {
  children?: ReactNode;
}

export default function AppViewport({ children }: Props) {
  useViewportBand();
  return (
    <div data-testid="app-viewport" className="app-viewport">
      {children}
    </div>
  );
}
