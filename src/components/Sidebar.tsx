import { CartoucheFrame, Icon, Pill, PlumbobMark, type IconName } from "./ui";
import { useApp } from "../state/AppContext";
import { PRODUCT_NAME } from "../lib/product";

export type Route =
  | "dashboard"
  | "library"
  | "creators"
  | "duplicates"
  | "conflicts"
  | "troubleshoot"
  | "quarantine"
  | "backups"
  | "activity"
  | "profiles"
  | "patchcenter"
  | "settings";

const NAV: { route: Route; label: string; icon: IconName }[] = [
  { route: "dashboard", label: "Dashboard", icon: "dashboard" },
  { route: "library", label: "Library", icon: "library" },
  { route: "creators", label: "Creators", icon: "profiles" },
  { route: "duplicates", label: "Duplicates", icon: "duplicates" },
  { route: "conflicts", label: "Conflicts", icon: "conflicts" },
  { route: "troubleshoot", label: "Troubleshoot", icon: "target" },
  { route: "quarantine", label: "Quarantine", icon: "quarantine" },
  { route: "backups", label: "Backups", icon: "backups" },
  { route: "activity", label: "Activity", icon: "activity" },
  { route: "profiles", label: "Profiles", icon: "profiles" },
  { route: "patchcenter", label: "Patch Center", icon: "calendar" },
  { route: "settings", label: "Settings", icon: "settings" },
];

/** Honest labeling: planned features are visible but clearly not built. */
const PLANNED: { label: string; icon: IconName }[] = [];

// Deterministic decoration, baked at build time from the approved preview.
const STARS = [
  { x: 150, y: 855, s: 3, o: 0.13, d: 1.8, c: "#f4dfa2" },
  { x: 185, y: 139, s: 4, o: 0.28, d: 0.1, c: "#cda45a" },
  { x: 234, y: 31, s: 3, o: 0.17, d: 4.3, c: "#f4dfa2" },
  { x: 12, y: 530, s: 7, o: 0.33, d: 0.3, c: "#e9cf8e" },
  { x: 190, y: 679, s: 3, o: 0.48, d: 5.8, c: "#f4dfa2" },
  { x: 190, y: 591, s: 5, o: 0.5, d: 1.9, c: "#e9cf8e" },
  { x: 244, y: 187, s: 9, o: 0.46, d: 2.7, c: "#cda45a" },
  { x: 85, y: 192, s: 3, o: 0.43, d: 4.0, c: "#cda45a" },
  { x: 102, y: 602, s: 3, o: 0.17, d: 2.6, c: "#cda45a" },
  { x: 143, y: 592, s: 5, o: 0.6, d: 2.0, c: "#f4dfa2" },
  { x: 149, y: 561, s: 5, o: 0.64, d: 2.7, c: "#e9cf8e" },
  { x: 134, y: 159, s: 3, o: 0.48, d: 0.7, c: "#f4dfa2" },
  { x: 123, y: 342, s: 3, o: 0.65, d: 3.6, c: "#f4dfa2" },
  { x: 87, y: 466, s: 2, o: 0.15, d: 2.5, c: "#e9cf8e" },
  { x: 85, y: 834, s: 4, o: 0.52, d: 5.8, c: "#cda45a" },
  { x: 31, y: 557, s: 9, o: 0.55, d: 2.1, c: "#f4dfa2" },
  { x: 253, y: 203, s: 2, o: 0.16, d: 2.8, c: "#e9cf8e" },
  { x: 237, y: 308, s: 4, o: 0.17, d: 3.3, c: "#cda45a" },
  { x: 88, y: 713, s: 4, o: 0.27, d: 2.5, c: "#e9cf8e" },
  { x: 229, y: 786, s: 6, o: 0.5, d: 2.5, c: "#e9cf8e" },
  { x: 213, y: 77, s: 4, o: 0.44, d: 2.6, c: "#f4dfa2" },
  { x: 28, y: 852, s: 5, o: 0.18, d: 3.8, c: "#f4dfa2" },
  { x: 222, y: 260, s: 3, o: 0.67, d: 0.5, c: "#cda45a" },
  { x: 60, y: 246, s: 9, o: 0.58, d: 5.9, c: "#f4dfa2" },
  { x: 4, y: 352, s: 7, o: 0.13, d: 5.7, c: "#cda45a" },
  { x: 207, y: 214, s: 2, o: 0.22, d: 2.7, c: "#f4dfa2" },
  { x: 43, y: 436, s: 5, o: 0.4, d: 4.1, c: "#cda45a" },
  { x: 92, y: 544, s: 5, o: 0.27, d: 6.0, c: "#e9cf8e" },
  { x: 33, y: 890, s: 9, o: 0.34, d: 0.1, c: "#f4dfa2" },
  { x: 212, y: 304, s: 6, o: 0.36, d: 4.8, c: "#f4dfa2" },
  { x: 176, y: 170, s: 7, o: 0.14, d: 5.3, c: "#cda45a" },
  { x: 246, y: 780, s: 5, o: 0.43, d: 4.1, c: "#f4dfa2" },
  { x: 113, y: 276, s: 3, o: 0.41, d: 0.7, c: "#f4dfa2" },
  { x: 63, y: 290, s: 9, o: 0.12, d: 0.2, c: "#e9cf8e" },
  { x: 128, y: 786, s: 9, o: 0.14, d: 4.8, c: "#f4dfa2" },
  { x: 104, y: 376, s: 3, o: 0.48, d: 4.7, c: "#e9cf8e" },
  { x: 262, y: 411, s: 6, o: 0.29, d: 0.5, c: "#cda45a" },
  { x: 88, y: 47, s: 3, o: 0.65, d: 1.1, c: "#e9cf8e" },
  { x: 71, y: 631, s: 3, o: 0.66, d: 1.8, c: "#cda45a" },
  { x: 153, y: 589, s: 2, o: 0.6, d: 3.6, c: "#cda45a" },
  { x: 54, y: 340, s: 2, o: 0.2, d: 0.6, c: "#e9cf8e" },
  { x: 107, y: 442, s: 6, o: 0.56, d: 1.3, c: "#cda45a" },
  { x: 225, y: 685, s: 6, o: 0.43, d: 1.3, c: "#cda45a" },
  { x: 161, y: 473, s: 3, o: 0.33, d: 5.9, c: "#f4dfa2" },
  { x: 22, y: 509, s: 7, o: 0.32, d: 1.3, c: "#cda45a" },
  { x: 84, y: 221, s: 6, o: 0.36, d: 0.4, c: "#f4dfa2" },
  { x: 220, y: 489, s: 3, o: 0.46, d: 2.5, c: "#e9cf8e" },
  { x: 163, y: 41, s: 5, o: 0.38, d: 1.0, c: "#f4dfa2" },
  { x: 257, y: 22, s: 7, o: 0.59, d: 1.1, c: "#f4dfa2" },
  { x: 99, y: 682, s: 7, o: 0.69, d: 2.7, c: "#e9cf8e" },
  { x: 202, y: 658, s: 7, o: 0.27, d: 4.4, c: "#f4dfa2" },
  { x: 201, y: 364, s: 5, o: 0.54, d: 0.9, c: "#e9cf8e" },
  { x: 239, y: 561, s: 3, o: 0.66, d: 2.5, c: "#cda45a" },
  { x: 76, y: 662, s: 5, o: 0.19, d: 0.1, c: "#cda45a" },
  { x: 108, y: 550, s: 4, o: 0.61, d: 4.7, c: "#f4dfa2" },
  { x: 139, y: 726, s: 5, o: 0.37, d: 5.5, c: "#cda45a" },
  { x: 153, y: 352, s: 2, o: 0.25, d: 4.0, c: "#cda45a" },
  { x: 80, y: 70, s: 3, o: 0.51, d: 0.7, c: "#f4dfa2" },
  { x: 130, y: 119, s: 3, o: 0.6, d: 5.8, c: "#e9cf8e" },
  { x: 246, y: 653, s: 2, o: 0.42, d: 1.6, c: "#cda45a" },
  { x: 217, y: 656, s: 2, o: 0.63, d: 1.2, c: "#cda45a" },
  { x: 38, y: 678, s: 3, o: 0.22, d: 1.3, c: "#e9cf8e" },
  { x: 74, y: 272, s: 2, o: 0.47, d: 2.3, c: "#e9cf8e" },
  { x: 217, y: 823, s: 2, o: 0.49, d: 0.8, c: "#e9cf8e" },
];
const C_POINTS: [number, number][] = [[98,669], [157,532], [150,432], [10,516], [28,475], [91,300], [179,743], [164,43], [221,178], [61,322], [213,869], [211,125], [44,169], [43,847], [157,693], [257,121], [165,866], [117,778], [137,209], [215,183]];
const C_LINES: [number, number, number, number][] = [[98,669,157,693], [98,669,117,778], [157,532,150,432], [157,532,157,693], [150,432,157,532], [150,432,28,475], [10,516,28,475], [10,516,157,532], [28,475,10,516], [28,475,150,432], [91,300,61,322], [91,300,137,209], [179,743,157,693], [179,743,117,778], [164,43,211,125], [164,43,257,121], [221,178,215,183], [221,178,211,125], [61,322,91,300], [61,322,44,169], [213,869,165,866], [213,869,179,743], [211,125,257,121], [211,125,215,183], [44,169,137,209], [44,169,61,322], [43,847,165,866], [43,847,117,778], [157,693,179,743], [157,693,98,669], [257,121,211,125], [257,121,221,178], [165,866,213,869], [165,866,117,778], [117,778,179,743], [117,778,157,693], [137,209,215,183], [137,209,221,178], [215,183,221,178], [215,183,211,125]];

function CrystalWatermark(props: { style: React.CSSProperties }) {
  return (
    <svg
      viewBox="0 0 24 30"
      fill="none"
      stroke="#efd79a"
      strokeWidth={0.7}
      aria-hidden="true"
      className="pointer-events-none absolute"
      style={props.style}
    >
      <path d="M12 1l9 10-9 18L3 11l9-10z" />
      <path d="M3 11h18M12 1v28M12 1L7 11l5 18M12 1l5 10-5 18" />
    </svg>
  );
}

export function Sidebar(props: {
  route: Route;
  onNavigate: (route: Route) => void;
}) {
  const { counts, duplicates, conflicts, troubleshoot, isGameRunning, info } =
    useApp();

  const badge = (route: Route): number | null => {
    if (route === "duplicates" && duplicates.openGroups > 0)
      return duplicates.openGroups;
    if (route === "conflicts" && conflicts.needsLook > 0)
      return conflicts.needsLook;
    if (route === "troubleshoot" && troubleshoot) return troubleshoot.poolSize;
    if (route === "quarantine" && counts && counts.quarantined > 0)
      return counts.quarantined;
    return null;
  };

  // The name is one centralized literal; the lockup splits it visually.
  const [brandFirst, ...brandRest] = PRODUCT_NAME.split(" ");

  return (
    <aside className="ml-sidebar relative flex h-full w-[270px] shrink-0 flex-col rounded-2xl">
      <CartoucheFrame large finials />
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-0 overflow-hidden rounded-2xl"
      >
        <span className="lattice" />
        <svg
          className="absolute inset-0 opacity-[0.16]"
          width="266"
          height="880"
          viewBox="0 0 266 880"
          aria-hidden="true"
        >
          <g stroke="#e9cf8e" strokeWidth={0.5} opacity={0.85}>
            {C_LINES.map(([x1, y1, x2, y2], i) => (
              <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} />
            ))}
          </g>
          {C_POINTS.map(([x, y], i) => (
            <circle key={i} cx={x} cy={y} r={1.4} fill="#e9cf8e" />
          ))}
        </svg>
        {STARS.map((s, i) =>
          s.s <= 4 ? (
            <i
              key={i}
              className="absolute rounded-full motion-safe:animate-[ml-twinkle_5s_ease-in-out_infinite_alternate]"
              style={{
                left: s.x,
                top: s.y,
                width: s.s,
                height: s.s,
                opacity: s.o,
                background: s.c,
                animationDelay: `${s.d}s`,
              }}
            />
          ) : (
            <svg
              key={i}
              className="absolute motion-safe:animate-[ml-twinkle_5s_ease-in-out_infinite_alternate]"
              style={{ left: s.x, top: s.y, opacity: s.o, animationDelay: `${s.d}s` }}
              width={s.s * 2}
              height={s.s * 2}
              viewBox="0 0 24 24"
            >
              <path
                d="M12 2l2.2 7.8L22 12l-7.8 2.2L12 22l-2.2-7.8L2 12l7.8-2.2L12 2z"
                fill={s.c}
              />
            </svg>
          )
        )}
        <CrystalWatermark
          style={{ right: -34, bottom: 110, width: 170, height: 212, opacity: 0.09 }}
        />
        <CrystalWatermark
          style={{ left: -28, top: 330, width: 110, height: 138, opacity: 0.06 }}
        />
        <span className="sweep" />
        <span className="grain-dark" />
      </div>

      <div className="relative flex flex-col items-center px-4 pb-3 pt-7 text-center">
        <span
          aria-hidden="true"
          className="pointer-events-none absolute left-1/2 top-14 h-[150px] w-[190px] -translate-x-1/2"
          style={{
            background:
              "radial-gradient(50% 50% at 50% 50%, rgba(80,225,150,.4), transparent 70%)",
          }}
        />
        <PlumbobMark size={104} />
        <div className="mt-2 font-display text-[26px] font-bold leading-tight text-sidebar-ink [text-shadow:0_2px_12px_rgba(0,0,0,0.6),0_0_26px_rgba(233,207,142,0.28)]">
          {brandFirst}
        </div>
        <div className="mt-1 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.4em] text-gold">
          <span aria-hidden="true" className="text-[9px] tracking-normal">✦</span>
          <span className="translate-x-[0.2em]">{brandRest.join(" ")}</span>
          <span aria-hidden="true" className="text-[9px] tracking-normal">✦</span>
        </div>
        <span
          aria-hidden="true"
          className="mt-4 h-px w-4/5 bg-gradient-to-r from-transparent via-gold/70 to-transparent shadow-[0_0_8px_rgba(224,192,121,0.5)]"
        />
      </div>

      <nav className="relative flex-1 space-y-0.5 overflow-y-auto px-3 py-1" aria-label="Main">
        {NAV.map((item) => {
          const active = props.route === item.route;
          const count = badge(item.route);
          return (
            <button
              key={item.route}
              type="button"
              onClick={() => props.onNavigate(item.route)}
              aria-current={active ? "page" : undefined}
              className={`flex w-full items-center justify-between rounded-control border border-transparent px-3.5 py-[11px] text-left font-display text-[15px] transition-all ${
                active
                  ? "nav-active font-semibold text-sidebar-ink"
                  : "text-sidebar-ink-muted hover:border-gold/70 hover:bg-white/5 hover:text-sidebar-ink hover:shadow-[0_0_0_1.4px_rgba(210,170,92,0.9),0_0_20px_rgba(210,170,92,0.5)]"
              }`}
            >
              <span className="flex items-center gap-3">
                <Icon
                  name={item.icon}
                  size={17}
                  className={
                    active
                      ? "text-gold drop-shadow-[0_0_5px_rgba(233,207,142,0.8)]"
                      : "opacity-80"
                  }
                />
                {item.label}
              </span>
              {count !== null ? <Pill tone="rose">{count}</Pill> : null}
            </button>
          );
        })}

        {PLANNED.length > 0 ? (

          <>

            <div className="pb-1 pt-4 text-[10px] font-bold uppercase tracking-[0.18em] text-sidebar-ink-muted">
          Planned
        </div>
        {PLANNED.map((item) => (
          <div
            key={item.label}
            className="flex w-full cursor-not-allowed items-center justify-between rounded-control px-3.5 py-[11px] font-display text-[15px] text-sidebar-ink-muted opacity-80"
            title="Not built yet — listed so the roadmap is honest, not to look finished."
          >
            <span className="flex items-center gap-3">
              <Icon name={item.icon} size={17} className="opacity-70" />
              {item.label}
            </span>
            <Pill tone="neutral">soon</Pill>
          </div>
        ))}

          </>

        ) : null}
      </nav>

      <div className="relative border-t border-gold/25 px-4 py-3 text-xs text-sidebar-ink-muted">
        <div className="flex items-center gap-2">
          <span
            aria-hidden="true"
            className={`h-2 w-2 rounded-full ${
              isGameRunning
                ? "bg-warning shadow-[0_0_10px_var(--warning)]"
                : "bg-success shadow-[0_0_10px_#4fce7f]"
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
