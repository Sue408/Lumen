import { useState } from "react";
import "./App.css";
import { AppShell } from "./app/AppShell";
import { PlaceholderPage } from "./app/PlaceholderPage";
import { navigationItems, type ViewId } from "./app/navigation";
import { useTheme } from "./app/useTheme";
import { WindowChrome } from "./components/WindowChrome";
import { UsagePage } from "./features/usage/UsagePage";
import { LogsPage } from "./features/logs/LogsPage";
import { KeysPage } from "./features/keys/KeysPage";
import { ProvidersPage } from "./features/providers/ProvidersPage";
import { RoutingPage } from "./features/routing/RoutingPage";
import { SettingsPage } from "./features/settings/SettingsPage";

function App() {
  const [view, setView] = useState<ViewId>("usage");
  const { theme, toggle } = useTheme();
  const currentLabel = navigationItems.find((item) => item.id === view)?.label ?? "";

  const renderView = () => {
    switch (view) {
      case "usage":
        return <UsagePage />;
      case "logs":
        return <LogsPage />;
      case "providers":
        return <ProvidersPage />;
      case "routing":
        return <RoutingPage />;
      case "keys":
        return <KeysPage />;
      case "settings":
        return <SettingsPage theme={theme} onToggleTheme={toggle} />;
      default:
        return <PlaceholderPage title={currentLabel} />;
    }
  };

  return (
    <>
      <AppShell activeView={view} onNavigate={setView}>
        {renderView()}
      </AppShell>
      <WindowChrome />
    </>
  );
}

export default App;
