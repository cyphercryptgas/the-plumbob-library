import { PlumbobMark, Pill } from "./ui";
import { useApp } from "../state/AppContext";
import { PRODUCT_NAME } from "../lib/product";

export type Route =
  | "dashboard"
  | "library"
  | "duplicates"
  | "quarantine"
  | "backups"
  | "activity"
  | "settings";

const NAV: { route: Route; label: string }[] = [
  { route: "dashboard", label: "Dashboard" },
  { route: "library", label: "Library" },
  { route: "duplicates", label: "Duplicates" },
  { route: "quarantine", label: "Quarantine" },
  { route: "backups", label: "Backups" },
  { route: "activity", label: "Activity" },
  { route: "settings", label: "Settings" },
];

/** Honest labeling: planned features are visible but clearly not built. */
const PLANNED = ["Conflicts", "Patch Center", "Profiles"];

export function Sidebar(props: {
  route: Route;
  onNavigate: (route: Route) => void;
}) {
  const { counts, duplicates, isGameRunning, info } = useApp();

  const badge = (route: Route): number | null => {
    if (route === "duplicates" && duplicates.openGroups > 0)
      return duplicates.openGroups;
    if (route === "quarantine" && counts && counts.quarantined > 0)
      return counts.quarantined;
    return null;
  };

  return (
    <aside className="flex h-full w-60 shrink-0 flex-col border-r border-border-subtle bg-sidebar">
      <div className="flex items-center gap-2.5 px-4 py-5">
        <PlumbobMark />
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-ink">
            {PRODUCT_NAME}
          </div>
          <div className="text-[11px] text-ink-muted">Mods, kept cozy</div>
        </div>
      </div>

      <nav className="flex-1 space-y-0.5 px-2" aria-label="Main">
        {NAV.map((item) => {
          const active = props.route === item.route;
          const count = badge(item.route);
          return (
            <button
              key={item.route}
              type="button"
              onClick={() => props.onNavigate(item.route)}
              aria-current={active ? "page" : undefined}
              className={`flex w-full items-center justify-between rounded-control px-3 py-2 text-left text-sm transition-colors ${
                active
                  ? "bg-surface font-semibold text-ink shadow-card"
                  : "text-ink-secondary hover:bg-soft hover:text-ink"
              }`}
            >
              <span>{item.label}</span>
              {count !== null ? <Pill tone="rose">{count}</Pill> : null}
            </button>
          );
        })}

        <div className="pb-1 pt-4 text-[11px] font-semibold uppercase tracking-wider text-ink-muted">
          Planned
        </div>
        {PLANNED.map((label) => (
          <div
            key={label}
            className="flex w-full cursor-not-allowed items-center justify-between rounded-control px-3 py-2 text-sm text-ink-muted"
            title="Not built yet — listed so the roadmap is honest, not to look finished."
          >
            <span>{label}</span>
            <Pill tone="neutral">soon</Pill>
          </div>
        ))}
      </nav>

      <div className="border-t border-border-subtle px-4 py-3 text-xs text-ink-muted">
        <div className="flex items-center gap-2">
          <span
            aria-hidden="true"
            className={`h-2 w-2 rounded-full ${
              isGameRunning ? "bg-warning" : "bg-success"
            }`}
          />
          <span>
            {isGameRunning ? "The Sims 4 is running" : "Game is closed"}
          </span>
        </div>
        {info ? <div className="mt-1">v{info.version}</div> : null}
      </div>
    </aside>
  );
}
