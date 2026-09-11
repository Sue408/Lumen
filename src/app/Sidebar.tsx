import { navigationItems, type ViewId } from "./navigation";
import { useGatewayStatus } from "./useGatewayStatus";
import logoMark from "../assets/lumen-mark.png";

type SidebarProps = {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
};

export function Sidebar({ activeView, onNavigate }: SidebarProps) {
  const gateway = useGatewayStatus();
  const running = gateway.status?.running ?? false;
  const baseUrl = gateway.status?.baseUrl ?? "http://127.0.0.1:8787";
  const address = baseUrl.replace(/^https?:\/\//, "");
  const statusLabel = gateway.error
    ? "启动失败"
    : gateway.busy
      ? "切换中"
      : running
        ? "本机运行"
        : "网关已停止";
  const statusDetail = gateway.error
    ? gateway.error
    : gateway.busy
      ? "请稍候"
      : running
        ? address
        : "点击启动";
  const statusClass = gateway.error ? "is-error" : running ? "is-running" : "is-stopped";

  return (
    <aside className="sidebar" aria-label="主导航">
      <div className="brand">
        <img className="brand-logo" src={logoMark} alt="" aria-hidden="true" />
        <span>Lumen</span>
      </div>
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
          className={`runtime-status ${statusClass}`}
          type="button"
          role="switch"
          aria-checked={running}
          aria-label={`${statusLabel}，点击${running ? "停止" : "启动"}网关`}
          title={gateway.error ? `${statusLabel} · ${gateway.error}` : `${statusLabel} · ${baseUrl}`}
          disabled={gateway.busy}
          onClick={gateway.toggle}
        >
          <span
            className={`runtime-dot ${running ? "is-running" : "is-stopped"}${gateway.error ? " is-error" : ""}`}
            aria-hidden="true"
          />
          <span className="runtime-text">
            <span className="runtime-label">{statusLabel}</span>
            <span className="runtime-detail">{statusDetail}</span>
          </span>
        </button>
      </div>
    </aside>
  );
}
