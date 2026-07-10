import { useCallback, useEffect, useState } from "react";
import * as api from "../lib/commands";
import { formatDateTime, plural } from "../lib/format";
import type { QuarantineView } from "../lib/types";
import { useApp } from "../state/AppContext";
import { Banner, Button, Card, EmptyState, Pill, Toggle } from "../components/ui";

export function Quarantine() {
  const { libraryVersion, isGameRunning, reportError } = useApp();
  const [entries, setEntries] = useState<QuarantineView[]>([]);
  const [includeRestored, setIncludeRestored] = useState(false);
  const [loading, setLoading] = useState(true);
  const [confirming, setConfirming] = useState<number | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    let alive = true;
    setLoading(true);
    api
      .listQuarantine(includeRestored)
      .then((data) => alive && setEntries(data))
      .catch(reportError)
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [includeRestored, reportError]);

  useEffect(() => load(), [load, libraryVersion]);

  const restore = useCallback(
    async (entry: QuarantineView) => {
      setBusyId(entry.id);
      setError(null);
      setNotice(null);
      try {
        const path = await api.restoreQuarantinedFile(entry.id);
        setNotice(`Restored to ${path}`);
        setConfirming(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusyId(null);
      }
    },
    []
  );

  const active = entries.filter((e) => e.status === "quarantined");

  return (
    <div className="space-y-4">
      <Card>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="max-w-xl text-sm leading-relaxed text-ink-secondary">
            Files here were moved out of your Mods folder with a verified
            backup taken first. Restoring puts a file back in its exact
            original spot, re-checked by fingerprint — and it never overwrites
            anything that's appeared there since.
          </p>
          <div className="w-56">
            <Toggle
              checked={includeRestored}
              onChange={setIncludeRestored}
              label="Show restored entries"
            />
          </div>
        </div>
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
      {isGameRunning && active.length > 0 ? (
        <Banner tone="warning">
          The Sims 4 is running — restoring is paused until the game closes.
        </Banner>
      ) : null}

      {!loading && entries.length === 0 ? (
        <EmptyState
          title="Quarantine is empty"
          body={
            includeRestored
              ? "Nothing has been set aside yet."
              : "Nothing is currently set aside. Files you quarantine from the Library or Duplicate Center will wait here, fully restorable."
          }
        />
      ) : (
        <div className="space-y-3">
          {entries.map((entry) => (
            <Card key={entry.id}>
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 flex-1">
                  <div className="break-all text-sm font-medium text-ink">
                    {entry.originalPath}
                  </div>
                  <div className="mt-1 text-xs text-ink-muted">
                    {entry.reason} · set aside {formatDateTime(entry.quarantinedAt)}
                    {entry.restoredAt
                      ? ` · restored ${formatDateTime(entry.restoredAt)}`
                      : ""}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  {entry.status === "quarantined" ? (
                    <>
                      <Button
                        variant="quiet"
                        onClick={() =>
                          api
                            .revealInExplorer(entry.quarantinePath)
                            .catch(reportError)
                        }
                        title="Show the stored copy in your file manager"
                      >
                        Reveal
                      </Button>
                      {confirming === entry.id ? (
                        <>
                          <Button
                            variant="quiet"
                            onClick={() => setConfirming(null)}
                            disabled={busyId === entry.id}
                          >
                            Never mind
                          </Button>
                          <Button
                            onClick={() => void restore(entry)}
                            disabled={busyId === entry.id || isGameRunning}
                          >
                            {busyId === entry.id
                              ? "Restoring…"
                              : "Yes, restore it"}
                          </Button>
                        </>
                      ) : (
                        <Button
                          variant="soft"
                          onClick={() => setConfirming(entry.id)}
                          disabled={isGameRunning}
                        >
                          Restore…
                        </Button>
                      )}
                    </>
                  ) : (
                    <Pill tone="sage">restored</Pill>
                  )}
                </div>
              </div>
            </Card>
          ))}
          {!loading ? (
            <p className="text-center text-xs text-ink-muted">
              {plural(active.length, "file")} currently set aside
            </p>
          ) : null}
        </div>
      )}
    </div>
  );
}
