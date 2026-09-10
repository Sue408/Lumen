import { navigationItems, type ViewId } from "./navigation";

type SidebarProps = {
  activeView: ViewId;
  onNavigate: (view: ViewId) => void;
};

export function Sidebar({ activeView, onNavigate }: SidebarProps) {
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
      <div className="runtime-status" role="status">
        <span className="runtime-dot" aria-hidden="true" />
        本机运行
      </div>
    </aside>
  );
}
