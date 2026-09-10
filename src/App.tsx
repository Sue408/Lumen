import { useState } from "react";
import "./App.css";
import { AppShell } from "./app/AppShell";
import { PlaceholderPage } from "./app/PlaceholderPage";
import { navigationItems, type ViewId } from "./app/navigation";
import { WindowChrome } from "./components/WindowChrome";
import { UsagePage } from "./features/usage/UsagePage";

function App() {
  const [view, setView] = useState<ViewId>("usage");
  const currentLabel = navigationItems.find((item) => item.id === view)?.label ?? "";

  return (
    <>
      <AppShell activeView={view} onNavigate={setView}>
        {view === "usage" ? <UsagePage /> : <PlaceholderPage title={currentLabel} />}
      </AppShell>
      <WindowChrome />
    </>
  );
}

export default App;
