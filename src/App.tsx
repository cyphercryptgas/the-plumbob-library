import { useState } from "react";
import { Icon } from "./components/ui";
import { isTauri } from "./lib/tauri";
import { AppProvider, useApp } from "./state/AppContext";
import { Sidebar, type Route } from "./components/Sidebar";
import { Banner } from "./components/ui";
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

function Screen(props: {
  route: Route;
  onNavigate: (r: Route) => void;
  libSeed: { q: string; n: number };
}) {
  switch (props.route) {
    case "dashboard":
      return <Dashboard onNavigate={props.onNavigate} />;
    case "settings":
      return <Settings />;
    case "library":
      return (
        <Library
          key={props.libSeed.n}
          initialSearch={props.libSeed.q || undefined}
        />
      );
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
  const { loading, settings, error, clearError, scan, info } = useApp();
  const [route, setRoute] = useState<Route>("dashboard");
  const [searchDraft, setSearchDraft] = useState("");
  const [libSeed, setLibSeed] = useState({ q: "", n: 0 });

  const submitSearch = () => {
    const q = searchDraft.trim();
    if (!q) return;
    setLibSeed((s) => ({ q, n: s.n + 1 }));
    setSearchDraft("");
    setRoute("library");
  };

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
    <div className="flex h-full gap-5 bg-app p-4">
      <Sidebar route={route} onNavigate={setRoute} />
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="relative flex min-h-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto px-10 py-8">
            <div className="mb-6 flex items-start justify-between gap-6">
              <div className="min-w-0">
                {route === "dashboard" ? (
                  <>
                    <h1 className="font-display text-[40px] font-bold leading-tight text-ink [text-shadow:0_1px_0_#fff]">
                      {/* Profiles (Planned) will supply the name here. */}
                      Welcome back{" "}
                      <span
                        aria-hidden="true"
                        className="align-[6px] text-[22px] text-gold [text-shadow:0_0_14px_rgba(201,164,92,0.8)]"
                      >
                        ✦
                      </span>
                    </h1>
                    {info ? (
                      <p className="mt-1 font-display text-[15px] font-semibold italic text-[#6d7a66]">
                        {info.tagline}
                      </p>
                    ) : null}
                  </>
                ) : (
                  <h1 className="text-[32px] font-bold text-ink [text-shadow:0_1px_0_#fff]">
                    {TITLES[route]}
                  </h1>
                )}
              </div>
              <div className="flex shrink-0 items-center gap-3 pt-2">
                {scan.running ? (
                  <span className="whitespace-nowrap text-xs text-ink-muted">
                    Scan in progress…
                  </span>
                ) : null}
                <label className="flex w-[300px] items-center gap-2 rounded-full border border-border-strong bg-gradient-to-b from-[#fffef8] to-[#f9f1dd] px-4 py-2.5 shadow-[inset_0_1px_0_#fff,0_8px_20px_-12px_rgba(90,70,30,0.5)] transition-all focus-within:shadow-[inset_0_1px_0_#fff,0_0_0_1.4px_rgba(210,170,92,0.9),0_0_20px_rgba(210,170,92,0.45)]">
                  <Icon name="search" size={15} className="shrink-0 text-ink-muted" />
                  <input
                    value={searchDraft}
                    onChange={(e) => setSearchDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") submitSearch();
                    }}
                    placeholder="Search your mods…"
                    aria-label="Search your library"
                    className="w-full bg-transparent text-sm text-ink outline-none placeholder:text-ink-muted"
                  />
                </label>
              </div>
            </div>
          {error ? (
            <div className="mb-4">
              <Banner tone="danger" onDismiss={clearError}>
                {error}
              </Banner>
            </div>
          ) : null}
            <Screen route={route} onNavigate={setRoute} libSeed={libSeed} />
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
