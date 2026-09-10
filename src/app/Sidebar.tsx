import { navigationItems, type ViewId } from "./navigation";
import { useTheme } from "./useTheme";

type SidebarProps = {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
};

export function Sidebar({ activeView, onNavigate }: SidebarProps) {
  const { theme, toggle } = useTheme();
  const isDark = theme === "dark";

  return (
    <aside className="sidebar" aria-label="主导航">
      <div className="brand">Lumen</div>
      <nav className="navigation">
        {navigationItems.map((item) => (
          <button
            className={item.id === activeView ? "nav-item is-active" : "nav-item"}
            key={item.id}
            type="button"
            aria-current={item.id === activeView ? "page" : undefined}
            onClick={() => onNavigate(item.id)}
          >
            {item.label}
          </button>
        ))}
      </nav>
      <div className="sidebar-footer">
        <div className="runtime-status" role="status">
          <span className="runtime-dot" aria-hidden="true" />
          本机运行
        </div>
        <button
          className="icon-button theme-toggle"
          type="button"
          aria-label={isDark ? "切换到浅色主题" : "切换到深色主题"}
          aria-pressed={isDark}
          title={isDark ? "切换到浅色主题" : "切换到深色主题"}
          onClick={toggle}
        >
          {isDark ? (
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M20 14.5A8 8 0 0 1 9.5 4a7 7 0 1 0 10.5 10.5Z" />
            </svg>
          ) : (
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="12" cy="12" r="4" />
              <path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6l1.4 1.4M17 17l1.4 1.4M18.4 5.6 17 7M7 17l-1.4 1.4" />
            </svg>
          )}
        </button>
      </div>
    </aside>
  );
}
