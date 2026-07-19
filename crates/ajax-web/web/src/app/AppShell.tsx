import type { ReactNode } from "react";

interface Props {
  chrome: ReactNode;
  children: ReactNode;
  nav: ReactNode;
}

export default function AppShell({ chrome, children, nav }: Props) {
  return (
    <div data-testid="app-shell" className="app-shell">
      {chrome}
      <main data-testid="app-main" className="app-main">
        {children}
      </main>
      {nav}
    </div>
  );
}
