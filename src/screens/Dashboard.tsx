import { useEffect, useState } from "react";
import {
  listBackups,
  listConflicts,
  listDuplicateGroups,
  listOperations,
} from "../lib/commands";
import { formatBytes, formatDuration, plural } from "../lib/format";
import { useApp } from "../state/AppContext";
import type { Route } from "../components/Sidebar";
import type {
  BackupView,
  ConflictGroup,
  DuplicateGroupView,
  OperationView,
} from "../lib/types";
import {
  Button,
  CartoucheFrame,
  Card,
  EmptyState,
  Icon,
  Pill,
  SectionTitle,
  Stat,
  type IconName,
} from "../components/ui";

const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

const shortDate = (iso: string) =>
  new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric" });

// Deterministic night-sky for the dark hero — decoration, not data.
const HERO_STARS: { x: number; y: number; s: number; o: number; d: number }[] = [
  { x: 10, y: 124, s: 2, o: 0.53, d: 0.8 },
  { x: 26, y: 39, s: 2, o: 0.42, d: 4.3 },
  { x: 75, y: 41, s: 2, o: 0.48, d: 0.8 },
  { x: 278, y: 118, s: 6, o: 0.25, d: 3.6 },
  { x: 64, y: 9, s: 6, o: 0.32, d: 1.2 },
  { x: 204, y: 97, s: 3, o: 0.26, d: 1.3 },
  { x: 303, y: 173, s: 2, o: 0.63, d: 5.0 },
  { x: 186, y: 38, s: 3, o: 0.22, d: 4.8 },
  { x: 196, y: 11, s: 2, o: 0.52, d: 5.2 },
  { x: 337, y: 59, s: 3, o: 0.55, d: 4.4 },
  { x: 103, y: 88, s: 3, o: 0.39, d: 1.6 },
  { x: 154, y: 89, s: 6, o: 0.39, d: 3.9 },
  { x: 165, y: 58, s: 2, o: 0.54, d: 1.8 },
  { x: 94, y: 147, s: 4, o: 0.2, d: 1.2 },
  { x: 311, y: 99, s: 2, o: 0.48, d: 1.7 },
  { x: 160, y: 170, s: 3, o: 0.21, d: 1.2 },
  { x: 174, y: 117, s: 6, o: 0.54, d: 3.4 },
  { x: 64, y: 72, s: 2, o: 0.59, d: 3.8 },
  { x: 103, y: 55, s: 5, o: 0.23, d: 5.9 },
  { x: 81, y: 125, s: 2, o: 0.4, d: 4.7 },
  { x: 146, y: 99, s: 8, o: 0.47, d: 0.2 },
  { x: 61, y: 128, s: 3, o: 0.55, d: 3.6 },
  { x: 258, y: 10, s: 4, o: 0.39, d: 4.7 },
  { x: 244, y: 93, s: 3, o: 0.16, d: 2.3 },
  { x: 148, y: 118, s: 3, o: 0.65, d: 0.8 },
  { x: 294, y: 161, s: 2, o: 0.61, d: 5.3 },
  { x: 102, y: 80, s: 3, o: 0.43, d: 3.4 },
  { x: 266, y: 138, s: 6, o: 0.26, d: 1.2 },
  { x: 318, y: 122, s: 4, o: 0.51, d: 2.8 },
  { x: 72, y: 99, s: 2, o: 0.41, d: 2.1 },
];
const HERO_C_POINTS: [number, number][] = [[108,84], [204,158], [95,20], [70,118], [116,46], [135,107], [167,62], [227,88], [287,160], [134,78], [29,18], [251,167]];
const HERO_C_LINES: [number, number, number, number][] = [[108,84,134,78], [108,84,116,46], [204,158,251,167], [204,158,287,160], [95,20,116,46], [95,20,29,18], [70,118,108,84], [70,118,135,107], [116,46,108,84], [116,46,95,20], [135,107,134,78], [135,107,108,84], [167,62,134,78], [167,62,116,46], [227,88,167,62], [227,88,204,158], [287,160,251,167], [287,160,204,158], [134,78,135,107], [134,78,108,84], [29,18,95,20], [29,18,116,46], [251,167,287,160], [251,167,204,158]];

type Finding = {
  key: string;
  icon: IconName;
  title: string;
  meta: string;
  /** Real timestamp where one exists (duplicates: newest copy). */
  date: string | null;
  status: string;
  tone: "warn" | "ok";
  route: Route;
};

function opIcon(type: string): IconName {
  if (type.includes("scan")) return "package";
  if (type.includes("quarantine")) return "lock";
  if (type.includes("restore")) return "quarantine";
  if (type.includes("backup")) return "backups";
  return "activity";
}

function opTitle(op: OperationView): string {
  if (op.summary) return op.summary;
  const t = op.operationType.replace(/_/g, " ");
  return t.charAt(0).toUpperCase() + t.slice(1);
}

function ActionTile(props: {
  icon: IconName;
  label: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={props.onClick}
      disabled={props.disabled}
      className="gold-edge-card flex flex-col items-center gap-2 rounded-card px-2 py-4 font-display text-[13px] font-semibold text-ink-secondary transition-all hover:-translate-y-0.5 hover:text-ink hover:shadow-[0_0_0_1.4px_rgba(210,170,92,0.9),0_0_20px_rgba(210,170,92,0.45)] disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:translate-y-0 disabled:hover:shadow-none"
    >
      <span className="icon-chip flex h-10 w-10 items-center justify-center rounded-xl">
        <Icon name={props.icon} size={18} />
      </span>
      {props.label}
    </button>
  );
}

export function Dashboard(props: { onNavigate: (route: Route) => void }) {
  const {
    counts,
    duplicates,
    conflicts,
    troubleshoot,
    isGameRunning,
    scan,
    startScan,
    cancelScan,
    libraryVersion,
    reportError,
  } = useApp();

  const [latestBackup, setLatestBackup] = useState<BackupView | null>(null);
  const [conflictGroups, setConflictGroups] = useState<ConflictGroup[]>([]);
  const [dupGroups, setDupGroups] = useState<DuplicateGroupView[]>([]);
  const [recentOps, setRecentOps] = useState<OperationView[]>([]);

  useEffect(() => {
    let alive = true;
    Promise.all([
      listBackups(),
      listConflicts(),
      listDuplicateGroups(),
      listOperations(5),
    ])
      .then(([backups, cg, dg, ops]) => {
        if (!alive) return;
        setLatestBackup(backups[0] ?? null);
        setConflictGroups(cg);
        setDupGroups(dg);
        setRecentOps(ops);
      })
      .catch((e) => reportError(String(e)));
    return () => {
      alive = false;
    };
  }, [libraryVersion, reportError]);

  const attention: { label: string; tone: "warning" | "danger" | "rose" }[] = [];
  if (counts) {
    if (counts.missing > 0)
      attention.push({ label: `${plural(counts.missing, "missing file")}`, tone: "warning" });
    if (counts.zeroByte > 0)
      attention.push({ label: `${plural(counts.zeroByte, "zero-byte file")}`, tone: "warning" });
    if (counts.deepScripts > 0)
      attention.push({
        label: `${plural(counts.deepScripts, "script")} nested too deep`,
        tone: "danger",
      });
    if (duplicates.openGroups > 0)
      attention.push({
        label: `${plural(duplicates.openGroups, "duplicate group")}`,
        tone: "rose",
      });
    if (conflicts.needsLook > 0)
      attention.push({
        label: `${plural(conflicts.needsLook, "possible conflict")}`,
        tone: "rose",
      });
  }

  // Recent findings: real conflicts and duplicates, worth-a-look first.
  const findings: Finding[] = [];
  for (const g of conflictGroups.filter((g) => !g.likelyIntentional)) {
    if (findings.length >= 2) break;
    const winner = g.members[g.members.length - 1];
    const other = g.members[0];
    findings.push({
      key: `c-${winner.fileId}`,
      icon: "conflicts",
      title: fileName(winner.relativePath),
      meta: `Shares ${plural(g.sharedKeyCount, "resource")} with ${fileName(other.relativePath)} · loads last · presumptive winner`,
      date: null,
      status: "Needs a look",
      tone: "warn",
      route: "conflicts",
    });
  }
  for (const g of dupGroups) {
    if (findings.length >= 4) break;
    const name = g.members[0] ? fileName(g.members[0].relativePath) : `Group #${g.id}`;
    findings.push({
      key: `d-${g.id}`,
      icon: "duplicates",
      title: name,
      meta: `${plural(g.members.length, "identical copy", "identical copies")} · ${formatBytes(g.reclaimableBytes)} reclaimable`,
      date: g.members
        .map((m) => m.modifiedAtFs)
        .filter((v): v is string => v !== null)
        .sort()
        .pop() ?? null,
      status: "Review",
      tone: "ok",
      route: "duplicates",
    });
  }
  if (findings.length < 4 && counts && counts.unsupported > 0) {
    findings.push({
      key: "u",
      icon: "alert",
      title: plural(counts.unsupported, "unsupported file"),
      meta: "Tray files parked in Mods — they belong in the game's Tray folder",
      date: null,
      status: "Suggestion",
      tone: "warn",
      route: "library",
    });
  }

  if (!counts || counts.totalFiles === 0) {
    return (
      <EmptyState
        title="No library data yet"
        body="Run a scan to take a careful, read-only inventory of your Mods folder. Nothing is changed by scanning."
      >
        <Button onClick={() => void startScan("initial")} disabled={scan.running}>
          {scan.running ? "Scanning…" : "Scan my library"}
        </Button>
      </EmptyState>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center gap-2">
        {troubleshoot ? (
          <Pill tone="warning">
            Troubleshooting in progress — {troubleshoot.poolSize} suspects,
            round {troubleshoot.round}
          </Pill>
        ) : null}
        <Pill tone={isGameRunning ? "warning" : "sage"}>
          {isGameRunning
            ? "The Sims 4 is running — changes are paused"
            : "Game closed — safe to tidy"}
        </Pill>
        {attention.length === 0 ? (
          <Pill tone="sage">Nothing needs your attention right now ✧</Pill>
        ) : (
          attention.map((a) => (
            <Pill key={a.label} tone={a.tone}>
              {a.label}
            </Pill>
          ))
        )}
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_372px] items-start gap-6">
        {/* ------------------------------ left ------------------------------ */}
        <div className="min-w-0 space-y-6">
          <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
            <Stat
              icon="file"
              label="Total files"
              value={counts.totalFiles.toLocaleString()}
              sub={`${formatBytes(counts.totalBytes)} on disk`}
            />
            <Stat
              icon="package"
              label="Packages indexed"
              value={counts.packages.toLocaleString()}
              sub="fingerprinted · read-only"
            />
            <Stat
              icon="duplicates"
              label="Duplicates"
              value={duplicates.openGroups.toLocaleString()}
              sub={
                duplicates.openGroups > 0
                  ? `${formatBytes(duplicates.reclaimableBytes)} reclaimable`
                  : "library is tidy"
              }
            />
            <Stat
              icon="backups"
              label="Last backup"
              value={latestBackup ? shortDate(latestBackup.createdAt) : "—"}
              sub={
                latestBackup
                  ? `backup #${latestBackup.id} · ${latestBackup.status}`
                  : "no backups yet"
              }
            />
          </div>

          <Card finials>
            <div className="flex items-baseline justify-between gap-3">
              <div className="min-w-0 flex-1">
                <SectionTitle>Recent findings</SectionTitle>
              </div>
              <button
                type="button"
                onClick={() => props.onNavigate("conflicts")}
                className="shrink-0 text-xs font-semibold text-sage-deep hover:text-sage"
              >
                View all →
              </button>
            </div>
            {findings.length === 0 ? (
              <p className="text-sm text-ink-secondary">
                Nothing needs a look right now ✧
              </p>
            ) : (
              <div>
                {findings.map((f) => (
                  <button
                    key={f.key}
                    type="button"
                    onClick={() => props.onNavigate(f.route)}
                    className="flex w-full items-center gap-3 border-b border-gold/25 px-2 py-3 text-left transition-all last:border-0 hover:rounded-control hover:bg-gold/10 hover:shadow-[0_0_0_1.4px_rgba(210,170,92,0.7),0_0_16px_rgba(210,170,92,0.35)]"
                  >
                    <span className="icon-chip flex h-12 w-12 shrink-0 items-center justify-center rounded-xl">
                      <Icon name={f.icon} size={19} />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium text-ink">
                        {f.title}
                      </span>
                      <span className="block truncate text-xs text-ink-muted">
                        {f.meta}
                      </span>
                    </span>
                    {f.date ? (
                      <span className="w-20 shrink-0 text-right text-xs text-ink-secondary">
                        {shortDate(f.date)}
                      </span>
                    ) : null}
                    <span
                      className={`w-24 shrink-0 text-right text-xs font-semibold ${
                        f.tone === "warn" ? "text-warning" : "text-sage-deep"
                      }`}
                    >
                      {f.status}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </Card>

          <Card>
            <SectionTitle>Scanning</SectionTitle>
            {scan.running ? (
              <div className="space-y-3">
                <p className="text-sm text-ink-secondary">
                  {scan.progress
                    ? scan.progress.phase === "scanning"
                      ? `Reading your library — ${plural(scan.progress.filesSeen, "file")} (${formatBytes(scan.progress.bytesSeen)}) so far.`
                      : scan.progress.phase === "parsing"
                        ? `Indexing packages — ${scan.progress.hashed} of ${scan.progress.toHash}.`
                        : `Fingerprinting content — ${scan.progress.hashed} of ${scan.progress.toHash} files.`
                    : "Starting up…"}
                </p>
                <Button variant="quiet" onClick={() => void cancelScan()}>
                  Cancel scan
                </Button>
              </div>
            ) : (
              <div className="space-y-3">
                {scan.lastOutcome ? (
                  <p className="text-sm leading-relaxed text-ink-secondary">
                    Last scan this session:{" "}
                    <span className="font-medium text-ink">
                      {plural(scan.lastOutcome.newFiles, "new file")}
                    </span>
                    , {scan.lastOutcome.changedFiles} changed,{" "}
                    {scan.lastOutcome.missingFiles} newly missing,{" "}
                    {scan.lastOutcome.duplicateGroups} duplicate{" "}
                    {scan.lastOutcome.duplicateGroups === 1 ? "group" : "groups"}
                    {scan.lastOutcome.packagesParsed > 0
                      ? `, ${plural(scan.lastOutcome.packagesParsed, "package")} indexed`
                      : ""}{" "}
                    — finished in {formatDuration(scan.lastOutcome.durationMs)}
                    {scan.lastOutcome.cancelled ? " (cancelled early)" : ""}.
                  </p>
                ) : (
                  <p className="text-sm text-ink-secondary">
                    Fresh scans keep counts, duplicates, and missing-file
                    tracking accurate.
                  </p>
                )}
                <Button onClick={() => void startScan()}>Scan now</Button>
              </div>
            )}
          </Card>
        </div>

        {/* ------------------------------ right ----------------------------- */}
        <div className="space-y-6">
          <div className="dark-card relative rounded-card">
            <CartoucheFrame finials />
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 overflow-hidden rounded-card"
            >
              <span className="lattice" />
              <svg
                className="absolute inset-0 opacity-[0.2]"
                width="360"
                height="195"
                viewBox="0 0 360 195"
                aria-hidden="true"
              >
                <g stroke="#e9cf8e" strokeWidth={0.5} opacity={0.85}>
                  {HERO_C_LINES.map(([x1, y1, x2, y2], i) => (
                    <line key={i} x1={x1} y1={y1} x2={x2} y2={y2} />
                  ))}
                </g>
                {HERO_C_POINTS.map(([x, y], i) => (
                  <circle key={i} cx={x} cy={y} r={1.4} fill="#e9cf8e" />
                ))}
              </svg>
              {HERO_STARS.map((s, i) =>
                s.s <= 3 ? (
                  <i
                    key={i}
                    className="absolute rounded-full bg-[#e9cf8e] motion-safe:animate-[ml-twinkle_5s_ease-in-out_infinite_alternate]"
                    style={{
                      left: s.x,
                      top: s.y,
                      width: s.s,
                      height: s.s,
                      opacity: s.o,
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
                    aria-hidden="true"
                  >
                    <path
                      d="M12 2l2.2 7.8L22 12l-7.8 2.2L12 22l-2.2-7.8L2 12l7.8-2.2L12 2z"
                      fill="#e9cf8e"
                    />
                  </svg>
                )
              )}
              <svg
                viewBox="0 0 24 30"
                fill="none"
                stroke="#efd79a"
                strokeWidth={0.7}
                className="absolute -right-4 -top-2 h-[150px] w-[118px] opacity-[0.13]"
              >
                <path d="M12 1l9 10-9 18L3 11l9-10z" />
                <path d="M3 11h18M12 1v28M12 1L7 11l5 18M12 1l5 10-5 18" />
              </svg>
              <span className="sweep" />
              <span className="grain-dark" />
            </div>
            <div className="relative p-5 text-[#efe7cd]">
              <div className="text-[11px] font-bold uppercase tracking-[0.16em] text-[#d5bd7c] [text-shadow:0_0_10px_rgba(213,189,124,0.4)]">
                Library size
              </div>
              <div className="mt-1 font-display text-[38px] font-bold leading-tight text-[#fdf6e0] [text-shadow:0_0_26px_rgba(233,207,142,0.45)]">
                {formatBytes(counts.totalBytes)}
              </div>
              <div className="mt-0.5 text-xs text-[#a3c3a3]">
                {counts.totalFiles.toLocaleString()} files · read-only index
              </div>
              <div className="mt-4 text-xs text-[#efd79a] [text-shadow:0_0_10px_rgba(233,207,142,0.5)]">
                ✦ {counts.packages.toLocaleString()} packages fingerprinted
              </div>
            </div>
          </div>

          <Card finials>
            <div className="flex items-baseline justify-between gap-3">
              <div className="min-w-0 flex-1">
                <SectionTitle>Recent activity</SectionTitle>
              </div>
              <button
                type="button"
                onClick={() => props.onNavigate("activity")}
                className="shrink-0 text-xs font-semibold text-sage-deep hover:text-sage"
              >
                View all →
              </button>
            </div>
            {recentOps.length === 0 ? (
              <p className="text-sm text-ink-secondary">
                No operations yet — everything so far has been read-only.
              </p>
            ) : (
              <div>
                {recentOps.slice(0, 3).map((op) => (
                  <div
                    key={op.id}
                    className="flex items-start gap-3 border-b border-gold/25 py-3 last:border-0"
                  >
                    <span className="icon-chip flex h-9 w-9 shrink-0 items-center justify-center rounded-lg">
                      <Icon name={opIcon(op.operationType)} size={15} />
                    </span>
                    <span className="min-w-0">
                      <span className="block truncate text-[13px] font-medium text-ink">
                        {opTitle(op)}
                      </span>
                      <span className="block text-[11.5px] text-ink-muted">
                        {shortDate(op.createdAt)} · {op.status}
                      </span>
                    </span>
                  </div>
                ))}
              </div>
            )}
          </Card>

          <Card finials>
            <SectionTitle>Quick actions</SectionTitle>
            <div className="grid grid-cols-2 gap-3">
              <ActionTile
                icon="library"
                label={scan.running ? "Scanning…" : "Scan now"}
                onClick={() => void startScan()}
                disabled={scan.running}
              />
              <ActionTile
                icon="duplicates"
                label="Review duplicates"
                onClick={() => props.onNavigate("duplicates")}
              />
              <ActionTile
                icon="backups"
                label="Open backups"
                onClick={() => props.onNavigate("backups")}
              />
              <ActionTile
                icon="activity"
                label="View activity"
                onClick={() => props.onNavigate("activity")}
              />
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
}
