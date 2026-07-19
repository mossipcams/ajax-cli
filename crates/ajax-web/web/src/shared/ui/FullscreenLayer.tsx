import type { ReactNode } from "react";

interface Props {
  children?: ReactNode;
  zIndex?: number;
}

export default function FullscreenLayer({ children, zIndex = 50 }: Props) {
  return (
    <div data-testid="fullscreen-layer" className="fullscreen-layer" style={{ zIndex }}>
      {children}
    </div>
  );
}
