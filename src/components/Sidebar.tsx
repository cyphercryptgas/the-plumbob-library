import { Icon, Pill, PlumbobMark, type IconName } from "./ui";
import { useApp } from "../state/AppContext";
import { PRODUCT_NAME } from "../lib/product";

export type Route =
  | "dashboard"
  | "library"
  | "duplicates"
  | "conflicts"
  | "quarantine"
  | "backups"
  | "activity"
  | "settings";

const NAV: { route: Route; label: string; icon: IconName }[] = [
  { route: "dashboard", label: "Dashboard", icon: "dashboard" },
  { route: "library", label: "Library", icon: "library" },
  { route: "duplicates", label: "Duplicates", icon: "duplicates" },
  { route: "conflicts", label: "Conflicts", icon: "conflicts" },
  { route: "quarantine", label: "Quarantine", icon: "quarantine" },
  { route: "backups", label: "Backups", icon: "backups" },
  { route: "activity", label: "Activity", icon: "activity" },
  { route: "settings", label: "Settings", icon: "settings" },
];

/** Honest labeling: planned features are visible but clearly not built. */
const PLANNED: { label: string; icon: IconName }[] = [
  { label: "Patch Center", icon: "calendar" },
  { label: "Profiles", icon: "profiles" },
];

export function Sidebar(props: {
  route: Route;
  onNavigate: (route: Route) => void;
}) {
  const { counts, duplicates, conflicts, isGameRunning, info } = useApp();

  const badge = (route: Route): number | null => {
    if (route === "duplicates" && duplicates.openGroups > 0)
      return duplicates.openGroups;
    if (route === "conflicts" && conflicts.needsLook > 0)
      return conflicts.needsLook;
    if (route === "quarantine" && counts && counts.quarantined > 0)
      return counts.quarantined;
    return null;
  };

  // The name is one centralized literal; the lockup splits it visually.
  const [brandFirst, ...brandRest] = PRODUCT_NAME.split(" ");

  return (
    <aside className="ml-sidebar relative flex h-full w-60 shrink-0 flex-col">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-2 rounded-xl border border-gold/25"
      />
      <Icon
        name="sparkle"
        size={11}
        className="pointer-events-none absolute right-8 top-9 text-gold/50"
      />
      <Icon
        name="sparkle"
        size={8}
        className="pointer-events-none absolute left-7 top-24 text-gold/40"
      />
      <Icon
        name="sparkle"
        size={7}
        className="pointer-events-none absolute right-12 top-44 text-gold/30"
      />

      <div className="relative flex flex-col items-center px-4 pb-4 pt-7 text-center">
        <PlumbobMark size={92} />
        <div className="mt-2 font-display text-[22px] font-semibold leading-tight text-sidebar-ink">
          {brandFirst}
        </div>
        <div className="mt-0.5 text-[11px] font-semibold uppercase tracking-[0.4em] text-gold">
          {brandRest.join(" ")}
        </div>
        <span
          aria-hidden="true"
          className="mt-4 h-px w-4/5 bg-gradient-to-r from-transparent via-gold/50 to-transparent"
        />
      </div>

      <nav className="relative flex-1 space-y-0.5 overflow-y-auto px-3" aria-label="Main">
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
                  ? "nav-active font-semibold text-sidebar-ink"
                  : "text-sidebar-ink-muted hover:bg-sidebar-hover hover:text-sidebar-ink"
              }`}
            >
              <span className="flex items-center gap-2.5">
                <Icon
                  name={item.icon}
                  size={17}
                  className={active ? "text-gold" : "opacity-80"}
                />
                {item.label}
              </span>
              {count !== null ? <Pill tone="rose">{count}</Pill> : null}
            </button>
          );
        })}

        <div className="pb-1 pt-4 text-[11px] font-semibold uppercase tracking-wider text-sidebar-ink-muted">
          Planned
        </div>
        {PLANNED.map((item) => (
          <div
            key={item.label}
            className="flex w-full cursor-not-allowed items-center justify-between rounded-control px-3 py-2 text-sm text-sidebar-ink-muted opacity-80"
            title="Not built yet — listed so the roadmap is honest, not to look finished."
          >
            <span className="flex items-center gap-2.5">
              <Icon name={item.icon} size={17} className="opacity-70" />
              {item.label}
            </span>
            <Pill tone="neutral">soon</Pill>
          </div>
        ))}
      </nav>

      <div className="relative border-t border-sidebar-hover px-4 py-3 text-xs text-sidebar-ink-muted">
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
