import { useCallback, useEffect, useState } from "react";
import * as api from "../lib/commands";
import { formatBytes, formatDateTime, plural, shortHash } from "../lib/format";
import type { BackupEntryView, BackupView } from "../lib/types";
import { useApp } from "../state/AppContext";
import { Banner, Button, Card, EmptyState, Pill } from "../components/ui";

export function Backups() {
  const { libraryVersion, isGameRunning, reportError } = useApp();
  const [backups, setBackups] = useState<BackupView[]>([]);
  const [loading, setLoading] = useState(true);
  const [openId, setOpenId] = useState<number | null>(null);
  const [entries, setEntries] = useState<Record<number, BackupEntryView[]>>({});
  /** source paths that hit an occupied destination and now offer overwrite */
  const [occupied, setOccupied] = useState<Set<string>>(new Set());
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    api
      .listBackups()
      .then((data) => alive && setBackups(data))
      .catch(reportError)
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [libraryVersion, reportError]);

  const toggleOpen = useCallback(
    async (backupId: number) => {
      if (openId === backupId) {
        setOpenId(null);
        return;
      }
      setOpenId(backupId);
      if (!entries[backupId]) {
        try {
          const list = await api.listBackupEntries(backupId);
          setEntries((e) => ({ ...e, [backupId]: list }));
        } catch (e) {
          reportError(e);
        }
      }
    },
    [openId, entries, reportError]
  );

  const restore = useCallback(
    async (backupId: number, sourcePath: string, overwrite: boolean) => {
      const key = `${backupId}:${sourcePath}`;
      setBusyKey(key);
      setError(null);
      setNotice(null);
      try {
        const path = await api.restoreBackupEntry(backupId, sourcePath, overwrite);
        setNotice(`Restored to ${path}`);
        setOccupied((s) => {
          const next = new Set(s);
          next.delete(key);
          return next;
        });
      } catch (e) {
        const message = e instanceof Error ? e.message : String(e);
        if (message.includes("already occupied")) {
          setOccupied((s) => new Set(s).add(key));
        } else {
          setError(message);
        }
      } finally {
        setBusyKey(null);
      }
    },
    []
  );

  return (
    <div className="space-y-4">
      <Card>
        <p className="max-w-xl text-sm leading-relaxed text-ink-secondary">
          Every time this app is about to change files, it takes a verified
          snapshot first — automatically. Each one can be browsed and restored
          file by file. A corrupted backup copy refuses to restore rather than
          replacing a live file with damaged bytes.
        </p>
      </Card>

      {notice ? (
        <Banner tone="success" onDismiss={() => setNotice(null)}>
          {notice}
        </Banner>
      ) : null}
      {error ? (
        <Banner tone="danger" onDismiss={() => setError(null)}>
          {error}
        </Banner>
      ) : null}

      {!loading && backups.length === 0 ? (
        <EmptyState
          title="No backups yet"
          body="Snapshots appear here automatically the first time you set files aside or run any other change."
        />
      ) : (
        <div className="space-y-3">
          {backups.map((backup) => {
            const isOpen = openId === backup.id;
            const list = entries[backup.id];
            return (
              <Card key={backup.id}>
                <button
                  type="button"
                  className="flex w-full flex-wrap items-center justify-between gap-3 text-left"
                  onClick={() => void toggleOpen(backup.id)}
                  aria-expanded={isOpen}
                >
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium text-ink">
                      {backup.reason}
                    </div>
                    <div className="mt-0.5 text-xs text-ink-muted">
                      {formatDateTime(backup.createdAt)} ·{" "}
                      {plural(backup.totalFiles, "file")} ·{" "}
                      {formatBytes(backup.totalBytes)}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <Pill tone={backup.status === "available" ? "sage" : "neutral"}>
                      {backup.status}
                    </Pill>
                    <span className="text-xs text-ink-muted">
                      {isOpen ? "▴" : "▾"}
                    </span>
                  </div>
                </button>

                {isOpen ? (
                  <div className="mt-3 border-t border-border-subtle pt-3">
                    <div className="mb-2 flex justify-end">
                      <Button
                        variant="quiet"
                        onClick={() =>
                          api.revealInExplorer(backup.rootPath).catch(reportError)
                        }
                      >
                        Reveal backup folder
                      </Button>
                    </div>
                    {!list ? (
                      <p className="text-sm text-ink-muted">Loading contents…</p>
                    ) : (
                      <ul className="space-y-2">
                        {list.map((entry) => {
                          const key = `${backup.id}:${entry.sourcePath}`;
                          const needsOverwrite = occupied.has(key);
                          const busy = busyKey === key;
                          return (
                            <li
                              key={entry.sourcePath}
                              className="flex flex-wrap items-center justify-between gap-2 rounded-control bg-soft px-3 py-2"
                            >
                              <div className="min-w-0 flex-1">
                                <div className="break-all text-sm text-ink">
                                  {entry.sourcePath}
                                </div>
                                <div className="text-xs text-ink-muted">
                                  {formatBytes(entry.sizeBytes)} · fingerprint{" "}
                                  {shortHash(entry.sha256)}
                                </div>
                              </div>
                              {needsOverwrite ? (
                                <div className="flex items-center gap-2">
                                  <span className="text-xs text-warning">
                                    A file already lives there.
                                  </span>
                                  <Button
                                    variant="danger"
                                    disabled={busy || isGameRunning}
                                    onClick={() =>
                                      void restore(backup.id, entry.sourcePath, true)
                                    }
                                  >
                                    {busy ? "Restoring…" : "Replace it"}
                                  </Button>
                                </div>
                              ) : (
                                <Button
                                  variant="soft"
                                  disabled={busy || isGameRunning}
                                  title={
                                    isGameRunning
                                      ? "Close The Sims 4 first"
                                      : undefined
                                  }
                                  onClick={() =>
                                    void restore(backup.id, entry.sourcePath, false)
                                  }
                                >
                                  {busy ? "Restoring…" : "Restore"}
                                </Button>
                              )}
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </div>
                ) : null}
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
