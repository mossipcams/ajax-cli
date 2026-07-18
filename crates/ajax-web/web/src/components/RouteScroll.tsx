import type { ReactNode } from "react";

interface Props {
  children?: ReactNode;
}

export default function RouteScroll({ children }: Props) {
  return (
    <div data-testid="route-scroll" className="route-scroll">
      {children}
    </div>
  );
}
