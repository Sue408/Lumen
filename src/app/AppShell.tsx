import type { ReactNode } from "react";
import { Sidebar } from "./Sidebar";
import type { ViewId } from "./navigation";

type AppShellProps = {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
  children: ReactNode;
};

export function AppShell({ activeView, onNavigate, children }: AppShellProps) {
  return (
    <div className="app-shell">
      <Sidebar activeView={activeView} onNavigate={onNavigate} />
      {children}
    </div>
  );
}
