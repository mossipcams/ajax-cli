import type { ReactNode } from "react";

interface Props {
  chrome: ReactNode;
  children: ReactNode;
  nav: ReactNode;
  className?: string;
}

export default function AppShell({ chrome, children, nav, className }: Props) {
  const shellClass = className ? `app-shell ${className}` : "app-shell";
  return (
    <div data-testid="app-shell" className={shellClass}>
      {chrome}
      <main data-testid="app-main" className="app-main">
        {children}
      </main>
      {nav}
    </div>
  );
}
