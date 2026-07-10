import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "../lib/commands";
import { asMessage } from "../lib/tauri";
import { formatBytes, plural } from "../lib/format";
import { PRODUCT_NAME, PRODUCT_TAGLINE, AFFILIATION_DISCLAIMER } from "../lib/product";
import type { ModsFolderCheck } from "../lib/types";
import { useApp } from "../state/AppContext";
import { Banner, Button, Card, PlumbobMark } from "../components/ui";

/**
 * First-run setup: pick (or confirm) the Mods folder, then run the first
 * scan. Nothing is written until "Set up my library" — and even then only
 * settings and read-only scan results.
 */
export function Onboarding() {
  const { settings, saveSettings, startScan, scan, refreshAll } = useApp();
  const [path, setPath] = useState<string>("");
  const [detecting, setDetecting] = useState(true);
  const [check, setCheck] = useState<ModsFolderCheck | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .detectModsFolder()
      .then((found) => {
        if (alive && found) setPath(found);
      })
      .catch(() => {})
      .finally(() => alive && setDetecting(false));
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    let alive = true;
    if (!path) {
      setCheck(null);
      return;
    }
    api
      .validateModsFolder(path)
      .then((result) => alive && setCheck(result))
      .catch(() => alive && setCheck(null));
    return () => {
      alive = false;
    };
  }, [path]);

  const browse = useCallback(async () => {
    try {
      const picked = await open({
        directory: true,
        title: "Choose your Sims 4 Mods folder",
      });
      if (typeof picked === "string") setPath(picked);
    } catch (e) {
      setError(asMessage(e));
    }
  }, []);

  const begin = useCallback(async () => {
    if (!settings) return;
    setBusy(true);
    setError(null);
    const saved = await saveSettings({ ...settings, modsFolder: path });
    if (!saved) {
      setBusy(false);
      return;
    }
    try {
      await startScan("initial");
      await refreshAll();
    } catch (e) {
      setError(asMessage(e));
    } finally {
      setBusy(false);
    }
  }, [settings, path, saveSettings, startScan, refreshAll]);

  const folderLooksRight =
    check?.isDirectory && (check.hasResourceCfg || check.hasSimsFiles);
  const canBegin = Boolean(path) && check?.isDirectory === true && !busy;

  return (
    <main className="flex min-h-full items-center justify-center bg-app p-8">
      <div className="w-full max-w-xl space-y-4">
        <Card>
          <div className="flex items-center gap-3">
            <PlumbobMark size={40} />
            <div>
              <h1 className="text-xl font-semibold text-ink">{PRODUCT_NAME}</h1>
              <p className="text-sm text-ink-secondary">{PRODUCT_TAGLINE}</p>
            </div>
          </div>
          <p className="mt-4 rounded-control bg-sage-soft p-3 text-sm leading-relaxed text-sage-deep">
            Setup is gentle: choose your Mods folder and this app takes a
            careful, read-only look around. Nothing is renamed, moved, or
            deleted during setup — ever.
          </p>
        </Card>

        <Card>
          <h2 className="text-sm font-semibold text-ink">Where do your mods live?</h2>
          <p className="mt-1 text-xs text-ink-muted">
            {detecting
              ? "Looking for the usual spot…"
              : path
                ? "Found a likely folder — double-check it's the right one."
                : "Couldn't auto-detect it (localized Documents folders hide it sometimes). Browse to it below."}
          </p>
          <div className="mt-3 flex items-center gap-2">
            <div className="min-w-0 flex-1 truncate rounded-control border border-border-subtle bg-soft px-3 py-2 text-sm text-ink" title={path || undefined}>
              {path || "No folder chosen yet"}
            </div>
            <Button variant="soft" onClick={() => void browse()}>
              Browse…
            </Button>
          </div>

          {check && path ? (
            <div className="mt-3 text-xs text-ink-secondary">
              {check.isDirectory ? (
                <>
                  <span className="font-medium text-ink">
                    {plural(check.topLevelEntries, "item")}
                  </span>{" "}
                  at the top level
                  {check.hasResourceCfg ? " · Resource.cfg present" : ""}
                  {check.hasSimsFiles ? " · Sims content spotted" : ""}
                  {!folderLooksRight ? (
                    <span className="block pt-1 text-warning">
                      This folder doesn't obviously contain Sims content at the
                      top level. If it's the right one, that's okay — the scan
                      looks deeper.
                    </span>
                  ) : null}
                </>
              ) : (
                <span className="text-danger">
                  That path isn't a folder this app can open.
                </span>
              )}
            </div>
          ) : null}
        </Card>

        {scan.running ? (
          <Card>
            <h2 className="text-sm font-semibold text-ink">First look in progress…</h2>
            <p className="mt-1 text-sm text-ink-secondary">
              {scan.progress
                ? scan.progress.phase === "scanning"
                  ? `Reading your library — ${plural(scan.progress.filesSeen, "file")} (${formatBytes(scan.progress.bytesSeen)}) so far.`
                  : `Fingerprinting content — ${scan.progress.hashed} of ${scan.progress.toHash} files hashed.`
                : "Starting up…"}
            </p>
          </Card>
        ) : null}

        {error ? (
          <Banner tone="danger" onDismiss={() => setError(null)}>
            {error}
          </Banner>
        ) : null}

        <div className="flex items-center justify-between">
          <p className="max-w-xs text-[11px] leading-relaxed text-ink-muted">
            Backups and quarantine live in the app's own data folder by
            default — changeable later in Settings.
          </p>
          <Button onClick={() => void begin()} disabled={!canBegin}>
            {busy ? "Setting up…" : "Set up my library"}
          </Button>
        </div>

        <p className="text-center text-[10px] leading-relaxed text-ink-muted">
          {AFFILIATION_DISCLAIMER}
        </p>
      </div>
    </main>
  );
}
