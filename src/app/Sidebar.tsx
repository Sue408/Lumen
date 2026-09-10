import { navigationItems, type ViewId } from "./navigation";
import { useGatewayStatus } from "./useGatewayStatus";

type SidebarProps = {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
};

export function Sidebar({ activeView, onNavigate }: SidebarProps) {
  const gateway = useGatewayStatus();
  const running = gateway.status?.running ?? false;
  const statusLabel = gateway.error
    ? "启动失败"
    : gateway.busy
      ? "切换中"
      : running
        ? "网关运行中"
        : "网关已停止";

  return (
    <aside className="sidebar" aria-label="主导航">
      <div className="brand">Lumen</div>
      <nav className="navigation">
        {navigationItems.map((item) => {
          const Icon = item.icon;
          return (
            <button
              className={item.id === activeView ? "nav-item is-active" : "nav-item"}
              key={item.id}
              type="button"
              aria-current={item.id === activeView ? "page" : undefined}
              onClick={() => onNavigate(item.id)}
            >
              <Icon className="nav-icon" aria-hidden="true" />
              <span>{item.label}</span>
            </button>
          );
        })}
      </nav>
      <div className="sidebar-footer">
        <button
          className={`runtime-status${gateway.error ? " is-error" : ""}`}
          type="button"
          role="switch"
          aria-checked={running}
          aria-label={`${statusLabel}，点击${running ? "停止" : "启动"}网关`}
          title={`${statusLabel} · ${gateway.status?.baseUrl ?? "127.0.0.1:8787"}`}
          disabled={gateway.busy}
          onClick={gateway.toggle}
        >
          <span
            className={`runtime-dot ${running ? "is-running" : "is-stopped"}${gateway.error ? " is-error" : ""}`}
            aria-hidden="true"
          />
          {statusLabel}
        </button>
      </div>
    </aside>
  );
}
