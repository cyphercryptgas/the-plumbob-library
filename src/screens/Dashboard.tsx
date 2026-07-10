import { formatBytes, formatDuration, plural } from "../lib/format";
import { useApp } from "../state/AppContext";
import type { Route } from "../components/Sidebar";
import { Button, Card, EmptyState, Pill, SectionTitle, Stat } from "../components/ui";

export function Dashboard(props: { onNavigate: (route: Route) => void }) {
  const { counts, duplicates, isGameRunning, scan, startScan, cancelScan } = useApp();

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
  }

  return (
    <div className="space-y-5">
      <Card>
        <SectionTitle hint="A quick pulse on your library's wellbeing.">
          At a glance
        </SectionTitle>
        <div className="flex flex-wrap items-center gap-2">
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
      </Card>

      {counts && counts.totalFiles > 0 ? (
        <Card>
          <SectionTitle>Your library</SectionTitle>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Stat label="Files" value={counts.totalFiles.toLocaleString()} />
            <Stat label="Total size" value={formatBytes(counts.totalBytes)} />
            <Stat label="Packages" value={counts.packages.toLocaleString()} />
            <Stat label="Scripts" value={counts.scripts.toLocaleString()} />
            <Stat
              label="Archives"
              value={counts.archives.toLocaleString()}
              sub="zip · rar · 7z"
            />
            <Stat label="Unsupported" value={counts.unsupported.toLocaleString()} />
            <Stat label="Quarantined" value={counts.quarantined.toLocaleString()} />
            <Stat label="Missing" value={counts.missing.toLocaleString()} />
          </div>
        </Card>
      ) : (
        <EmptyState
          title="No library data yet"
          body="Run a scan to take a careful, read-only inventory of your Mods folder. Nothing is changed by scanning."
        >
          <Button onClick={() => void startScan("initial")} disabled={scan.running}>
            {scan.running ? "Scanning…" : "Scan my library"}
          </Button>
        </EmptyState>
      )}

      {duplicates.openGroups > 0 ? (
        <Card>
          <SectionTitle hint="Exact copies found by content fingerprint — not just matching names.">
            Duplicates
          </SectionTitle>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-sm text-ink-secondary">
              <span className="font-semibold text-ink">
                {plural(duplicates.openGroups, "group")}
              </span>{" "}
              of identical files —{" "}
              <span className="font-semibold text-ink">
                {formatBytes(duplicates.reclaimableBytes)}
              </span>{" "}
              reclaimable, with a backup taken before anything moves.
            </p>
            <Button variant="soft" onClick={() => props.onNavigate("duplicates")}>
              Review duplicates →
            </Button>
          </div>
        </Card>
      ) : null}

      <Card>
        <SectionTitle hint="Scans are read-only: inventory, fingerprints, duplicate detection.">
          Scanning
        </SectionTitle>
        {scan.running ? (
          <div className="space-y-3">
            <p className="text-sm text-ink-secondary">
              {scan.progress
                ? scan.progress.phase === "scanning"
                  ? `Reading your library — ${plural(scan.progress.filesSeen, "file")} (${formatBytes(scan.progress.bytesSeen)}) so far.`
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
                {scan.lastOutcome.duplicateGroups === 1 ? "group" : "groups"} —
                finished in {formatDuration(scan.lastOutcome.durationMs)}
                {scan.lastOutcome.cancelled ? " (cancelled early)" : ""}
                {scan.lastOutcome.scanErrors > 0
                  ? ` · ${plural(scan.lastOutcome.scanErrors, "path")} couldn't be read`
                  : ""}
                {scan.lastOutcome.hashErrors > 0
                  ? ` · ${plural(scan.lastOutcome.hashErrors, "file")} couldn't be fingerprinted`
                  : ""}
                .
              </p>
            ) : (
              <p className="text-sm text-ink-secondary">
                Fresh scans keep counts, duplicates, and missing-file tracking
                accurate.
              </p>
            )}
            <Button onClick={() => void startScan()}>Scan now</Button>
          </div>
        )}
      </Card>
    </div>
  );
}
