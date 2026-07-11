import { useState } from "react";
import { isTauri } from "./lib/tauri";
import { AppProvider, useApp } from "./state/AppContext";
import { Sidebar, type Route } from "./components/Sidebar";
import { Banner, OrnamentalFrame } from "./components/ui";
import { BrowserNotice } from "./screens/BrowserNotice";
import { Onboarding } from "./screens/Onboarding";
import { Dashboard } from "./screens/Dashboard";
import { Settings } from "./screens/Settings";
import { Library } from "./screens/Library";
import { Duplicates } from "./screens/Duplicates";
import { Conflicts } from "./screens/Conflicts";
import { Quarantine } from "./screens/Quarantine";
import { Backups } from "./screens/Backups";
import { Activity } from "./screens/Activity";

const TITLES: Record<Route, string> = {
  dashboard: "Dashboard",
  library: "Library",
  duplicates: "Duplicate Center",
  conflicts: "Conflicts",
  quarantine: "Quarantine",
  backups: "Backups",
  activity: "Activity",
  settings: "Settings",
};

function Screen(props: { route: Route; onNavigate: (r: Route) => void }) {
  switch (props.route) {
    case "dashboard":
      return <Dashboard onNavigate={props.onNavigate} />;
    case "settings":
      return <Settings />;
    case "library":
      return <Library />;
    case "duplicates":
      return <Duplicates />;
    case "conflicts":
      return <Conflicts />;
    case "quarantine":
      return <Quarantine />;
    case "backups":
      return <Backups />;
    case "activity":
      return <Activity />;
  }
}

function Shell() {
  const { loading, settings, error, clearError, scan } = useApp();
  const [route, setRoute] = useState<Route>("dashboard");

  if (loading) {
    return (
      <main className="flex h-full items-center justify-center bg-app">
        <p className="text-sm text-ink-muted">Opening your library…</p>
      </main>
    );
  }

  if (!settings?.modsFolder) {
    return <Onboarding />;
  }

  return (
    <div className="flex h-full bg-app">
      <Sidebar route={route} onNavigate={setRoute} />
      <div className="flex min-w-0 flex-1 flex-col p-3">
        <div className="relative flex min-h-0 flex-1 flex-col">
          <OrnamentalFrame />
          <div className="min-h-0 flex-1 overflow-y-auto px-10 py-8">
            <div className="mb-5 flex items-baseline justify-between">
              <h1 className="text-[30px] font-semibold text-ink">
                {TITLES[route]}
              </h1>
              {scan.running ? (
                <span className="text-xs text-ink-muted">Scan in progress…</span>
              ) : null}
            </div>
          {error ? (
            <div className="mb-4">
              <Banner tone="danger" onDismiss={clearError}>
                {error}
              </Banner>
            </div>
          ) : null}
            <Screen route={route} onNavigate={setRoute} />
          </div>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  if (!isTauri()) {
    return <BrowserNotice />;
  }
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}
