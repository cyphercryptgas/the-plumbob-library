import { useEffect, useState } from "react";
import {
  checkCurseUpdates,
  curseStatus,
  openExternal,
} from "../lib/commands";
import { onPatchProgress, type PatchProgressEvent } from "../lib/events";
import { useApp } from "../state/AppContext";
import type { Route } from "../components/Sidebar";
import type { CurseStatusRow, PatchCheckSummary } from "../lib/types";
import { Button, Card, Icon, Pill, SectionTitle } from "../components/ui";

function shortDate(iso: string | null): string {
  if (!iso) return "";
  return new Date(iso).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

export function PatchCenter(props: { onNavigate: (route: Route) => void }) {
  const { settings, reportError } = useApp();
  const [rows, setRows] = useState<CurseStatusRow[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [checking, setChecking] = useState(false);
  const [progress, setProgress] = useState<PatchProgressEvent | null>(null);
  const [summary, setSummary] = useState<PatchCheckSummary | null>(null);
  const [showCurrent, setShowCurrent] = useState(false);

  const hasKey = Boolean(settings?.curseforgeApiKey?.trim());

  const refresh = async () => {
    try {
      setRows(await curseStatus());
    } catch (e) {
      reportError(e);
    } finally {
      setLoaded(true);
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => onPatchProgress(setProgress), []);

  const openMod = async (r: CurseStatusRow) => {
    // Prefer the CurseForge desktop app; fall back to the website when the
    // protocol has no handler on this machine.
    if (r.curseModId && r.latestFileId) {
      try {
        await openExternal(
          `curseforge://install?addonId=${r.curseModId}&fileId=${r.latestFileId}`
        );
        return;
      } catch {
        /* no app handler — fall through to the web */
      }
    }
    if (r.websiteUrl) {
      openExternal(r.websiteUrl).catch(reportError);
    } else {
      reportError("CurseForge didn't provide a page for this mod.");
    }
  };

  const check = async () => {
    setChecking(true);
    setSummary(null);
    try {
      setSummary(await checkCurseUpdates());
      await refresh();
    } catch (e) {
      reportError(e);
    } finally {
      setChecking(false);
      setProgress(null);
    }
  };

  const updates = rows.filter((r) => r.updateAvailable);
  const current = rows.filter((r) => r.modName && !r.updateAvailable);
  const unknown = rows.filter((r) => !r.modName);
  const checkedAt = rows.find((r) => r.checkedAt)?.checkedAt ?? null;

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <Card finials>
        <SectionTitle>Update radar</SectionTitle>
        <p className="text-sm leading-relaxed text-ink-secondary">
          Your library, checked against CurseForge itself. Each file is
          identified by CurseForge's own fingerprint — a hash of its bytes
          with whitespace stripped — and compared to the mod's latest
          release. Only anonymous fingerprints ever leave this machine; your
          key stays in the local database.
        </p>
        {!hasKey ? (
          <div className="mt-4 flex flex-wrap items-center gap-3">
            <Pill tone="warning">No CurseForge API key yet</Pill>
            <Button variant="quiet" onClick={() => props.onNavigate("settings")}>
              Add one in Settings → Connections
            </Button>
          </div>
        ) : (
          <div className="mt-4 flex flex-wrap items-center gap-3">
            <Button disabled={checking} onClick={() => void check()}>
              {checking ? "Checking…" : "Check for updates"}
            </Button>
            {checkedAt ? (
              <span className="text-xs text-ink-muted">
                Last checked {shortDate(checkedAt)}
              </span>
            ) : loaded ? (
              <span className="text-xs text-ink-muted">
                Never checked — the first run fingerprints your whole library
                once (a few minutes), then it's fast.
              </span>
            ) : null}
          </div>
        )}
        {checking && progress ? (
          <div className="mt-4">
            <div className="flex items-baseline justify-between text-xs text-ink-muted">
              <span>{progress.phase}…</span>
              <span>
                {progress.done.toLocaleString()} /{" "}
                {progress.total.toLocaleString()}
              </span>
            </div>
            <div className="raised-pill mt-1.5 h-2.5 overflow-hidden rounded-full border border-gold/50 bg-surface">
              <div
                className="h-full rounded-full transition-[width] duration-150"
                style={{
                  width: `${Math.round((progress.done / Math.max(progress.total, 1)) * 100)}%`,
                  backgroundImage: "var(--gold-grad-soft)",
                  boxShadow: "0 0 8px rgba(201,164,92,0.6)",
                }}
              />
            </div>
          </div>
        ) : null}
        {summary ? (
          <p className="mt-3 text-xs text-ink-muted">
            Checked {summary.eligible.toLocaleString()} files
            {summary.newlyFingerprinted > 0
              ? ` (${summary.newlyFingerprinted.toLocaleString()} newly fingerprinted)`
              : ""}
            : {summary.matched.toLocaleString()} known to CurseForge
            {summary.nameMatched > 0
              ? ` (${summary.nameMatched.toLocaleString()} by name)`
              : ""}
            , {summary.updates.toLocaleString()} with updates
            {summary.otherGame > 0
              ? ` (${summary.otherGame.toLocaleString()} cross-game fingerprint ${summary.otherGame === 1 ? "collision" : "collisions"} ignored)`
              : ""}
            .
          </p>
        ) : null}
        {summary && summary.rateLimited ? (
          <p className="mt-2 text-xs leading-relaxed text-warning">
            CurseForge rate-limited the name search partway — everything found
            so far is cached, so running Check again continues where it
            stopped.
          </p>
        ) : null}
        {summary && summary.corpusProbe === false ? (
          <p className="mt-2 text-xs leading-relaxed text-ink-muted">
            CurseForge's exact-match index doesn't cover The Sims 4 (their own
            fingerprints fail to match themselves), so the radar matches by
            name instead —{" "}
            {summary.nameMatched > 0
              ? `${summary.nameMatched.toLocaleString()} mods recognized this way, labeled ≈.`
              : "nothing confident enough was found this run."}
          </p>
        ) : summary && summary.matched === 0 && summary.corpusProbe === true ? (
          <p className="mt-2 text-xs leading-relaxed text-warning">
            Diagnosis: the exact matcher works for Sims 4, so these exact
            files were never uploaded to CurseForge — creators' Patreon and
            site builds differ byte-for-byte. Name matching also found nothing
            confident this run.
          </p>
        ) : null}
      </Card>

      {loaded && checkedAt ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
          {[
            { label: "Updates", value: updates.length, tone: "warn" },
            { label: "Up to date", value: current.length, tone: "ok" },
            { label: "On CurseForge", value: updates.length + current.length, tone: "ok" },
            { label: "Not on CurseForge", value: unknown.length, tone: "muted" },
          ].map((s) => (
            <div
              key={s.label}
              className="gold-edge-card rounded-card px-3 py-3 text-center"
            >
              <div
                className={`font-display text-[24px] font-bold leading-tight ${
                  s.tone === "warn" && s.value > 0
                    ? "text-warning"
                    : "text-ink"
                }`}
              >
                {s.value.toLocaleString()}
              </div>
              <div className="text-[10px] font-bold uppercase tracking-[0.12em] text-[#94875e]">
                {s.label}
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {updates.length > 0 ? (
        <Card finials>
          <SectionTitle>Updates available</SectionTitle>
          <div>
            {updates.map((r) => (
              <div
                key={r.fileId}
                className="flex items-center gap-3 border-b border-gold/25 px-2 py-3 last:border-0"
              >
                <span className="icon-chip flex h-11 w-11 shrink-0 items-center justify-center rounded-xl">
                  <Icon name="conflicts" size={18} />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium text-ink">
                    {r.modName}
                  </span>
                  <span className="block truncate text-xs text-ink-muted">
                    {r.matchKind === "name"
                      ? `Matched by name — latest ${r.latestFileName} · ${shortDate(r.latestFileDate)}, newer than your copy`
                      : `You have ${r.matchedFileName} · ${shortDate(r.matchedFileDate)} → latest ${r.latestFileName} · ${shortDate(r.latestFileDate)}`}
                  </span>
                </span>
                {r.matchKind === "name" ? (
                  <Pill
                    tone="neutral"
                    title={
                      r.confidence
                        ? `Approximate match by name · ${Math.round(r.confidence * 100)}% token overlap`
                        : "Approximate match by name"
                    }
                  >
                    ≈ name
                  </Pill>
                ) : null}
                {!r.enabled ? <Pill tone="neutral">off</Pill> : null}
                <Button
                  variant="quiet"
                  title="Opens in the CurseForge app when installed, otherwise the website."
                  onClick={() => void openMod(r)}
                >
                  Open Mod
                </Button>
              </div>
            ))}
          </div>
        </Card>
      ) : loaded && checkedAt ? (
        <Card finials>
          <SectionTitle>Updates available</SectionTitle>
          <p className="text-sm text-ink-secondary">
            Everything CurseForge recognizes is on its latest release. ✦
          </p>
        </Card>
      ) : null}

      {loaded && checkedAt && current.length > 0 ? (
        <Card>
          <div className="flex items-baseline justify-between gap-3">
            <div className="min-w-0 flex-1">
              <SectionTitle>Up to date on CurseForge</SectionTitle>
            </div>
            <button
              type="button"
              className="shrink-0 text-xs font-semibold text-sage-deep"
              onClick={() => setShowCurrent((v) => !v)}
            >
              {showCurrent ? "Hide" : `Show ${current.length}`}
            </button>
          </div>
          {showCurrent ? (
            <div>
              {current.map((r) => (
                <div
                  key={r.fileId}
                  className="flex items-center gap-3 border-b border-gold/25 px-2 py-2.5 last:border-0"
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm text-ink">
                      {r.modName}
                    </span>
                    <span className="block truncate text-xs text-ink-muted">
                      {r.currentFilename} · {shortDate(r.matchedFileDate)}
                    </span>
                  </span>
                  {r.websiteUrl ? (
                    <Button
                      variant="quiet"
                      onClick={() =>
                        void openExternal(r.websiteUrl as string).catch(
                          reportError
                        )
                      }
                    >
                      Open
                    </Button>
                  ) : null}
                </div>
              ))}
            </div>
          ) : (
            <p className="text-xs text-ink-muted">
              {current.length.toLocaleString()} mods match their latest
              CurseForge release.
            </p>
          )}
        </Card>
      ) : null}

      {loaded && checkedAt && unknown.length > 0 ? (
        <Card frame={false} className="border border-border-subtle">
          <p className="text-xs leading-relaxed text-ink-muted">
            {unknown.length.toLocaleString()} files aren't on CurseForge —
            perfectly normal for Patreon, Tumblr, and merged CC. The radar
            simply can't see updates for them.
          </p>
        </Card>
      ) : null}
    </div>
  );
}
