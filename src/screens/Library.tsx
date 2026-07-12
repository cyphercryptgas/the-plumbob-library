import { useCallback, useEffect, useMemo, useState } from "react";
import * as api from "../lib/commands";
import { formatBytes, formatDateTime, plural } from "../lib/format";
import type { FileRow, LibraryFilter } from "../lib/types";
import { useApp } from "../state/AppContext";
import { Button, Card, EmptyState, Pill, TextInput } from "../components/ui";
import { QuarantineDialog } from "../components/QuarantineDialog";

const PAGE_SIZE = 100;

const FILTERS: { key: LibraryFilter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "packages", label: "Packages" },
  { key: "scripts", label: "Scripts" },
  { key: "archives", label: "Archives" },
  { key: "zero-byte", label: "Zero-byte" },
  { key: "deep-scripts", label: "Deep scripts" },
  { key: "missing", label: "Missing" },
  { key: "quarantined", label: "Quarantined" },
  { key: "unreadable", label: "Unreadable" },
];

const TYPE_TONES: Record<string, "sage" | "blue" | "rose" | "neutral" | "warning"> = {
  package: "sage",
  ts4script: "blue",
  zip: "rose",
  rar: "rose",
  "7z": "rose",
  image: "neutral",
  document: "neutral",
  config: "neutral",
  unsupported: "warning",
};

export function Library(props: { initialSearch?: string }) {
  const { libraryVersion, reportError } = useApp();
  const [search, setSearch] = useState(props.initialSearch ?? "");
  const [query, setQuery] = useState(props.initialSearch?.trim() ?? "");
  const [filter, setFilter] = useState<LibraryFilter>("all");
  const [page, setPage] = useState(0);
  const [rows, setRows] = useState<FileRow[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [quarantining, setQuarantining] = useState<number[] | null>(null);

  // Debounce typing into the effective query.
  useEffect(() => {
    const id = window.setTimeout(() => {
      setQuery(search.trim());
      setPage(0);
    }, 300);
    return () => window.clearTimeout(id);
  }, [search]);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    Promise.all([
      api.listFiles({
        search: query || undefined,
        filter,
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      }),
      api.countFiles({ search: query || undefined, filter }),
    ])
      .then(([data, count]) => {
        if (!alive) return;
        setRows(data);
        setTotal(count);
        setSelected(new Set());
      })
      .catch(reportError)
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [query, filter, page, libraryVersion, reportError]);

  const selectableRows = useMemo(
    () => rows.filter((r) => !r.missing && r.status !== "quarantined"),
    [rows]
  );
  const allSelected =
    selectableRows.length > 0 &&
    selectableRows.every((r) => selected.has(r.id));

  const toggleAll = useCallback(() => {
    setSelected((prev) => {
      if (selectableRows.every((r) => prev.has(r.id)) && selectableRows.length > 0)
        return new Set();
      return new Set(selectableRows.map((r) => r.id));
    });
  }, [selectableRows]);

  const toggleOne = useCallback((id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const hasMore = total !== null ? page * PAGE_SIZE + rows.length < total : rows.length === PAGE_SIZE;
  const rangeStart = page * PAGE_SIZE + 1;
  const rangeEnd = page * PAGE_SIZE + rows.length;

  return (
    <div className="space-y-4">
      <Card>
        <div className="flex flex-wrap items-center gap-3">
          <div className="min-w-[220px] flex-1">
            <TextInput
              value={search}
              onChange={setSearch}
              placeholder="Search by path or name…"
              ariaLabel="Search files"
            />
          </div>
          {selected.size > 0 ? (
            <Button onClick={() => setQuarantining([...selected])}>
              Set aside {plural(selected.size, "file")}…
            </Button>
          ) : null}
          <span className="text-xs text-ink-muted">
            {loading
              ? "Loading…"
              : rows.length === 0
                ? "No matches"
                : total !== null
                  ? `Showing ${rangeStart}–${rangeEnd} of ${total.toLocaleString()}`
                  : `Showing ${rangeStart}–${rangeEnd}`}
          </span>
        </div>
        <div className="mt-3 flex flex-wrap gap-1.5" role="group" aria-label="Filter by status">
          {FILTERS.map((f) => (
            <button
              key={f.key}
              type="button"
              onClick={() => {
                setFilter(f.key);
                setPage(0);
              }}
              aria-pressed={filter === f.key}
              className={`rounded-control border px-2.5 py-1 text-xs transition-colors ${
                filter === f.key
                  ? "border-transparent bg-accent font-medium text-ink"
                  : "border-border-subtle text-ink-secondary hover:bg-soft"
              }`}
            >
              {f.label}
            </button>
          ))}
        </div>
      </Card>

      {rows.length === 0 && !loading ? (
        <EmptyState
          title={
            query || filter !== "all"
              ? "No files match"
              : "Nothing here yet"
          }
          body={
            query || filter !== "all"
              ? "Try a different filter or a shorter fragment of the path."
              : "Run a scan from the Dashboard to inventory your Mods folder."
          }
        />
      ) : (
        <Card className="overflow-hidden !p-0">
          <table className="w-full text-left text-sm">
            <thead className="border-b border-border-subtle bg-soft text-xs uppercase tracking-wide text-ink-muted">
              <tr>
                <th className="w-10 px-3 py-2">
                  <input
                    type="checkbox"
                    aria-label="Select all on this page"
                    checked={allSelected}
                    onChange={toggleAll}
                  />
                </th>
                <th className="px-3 py-2">File</th>
                <th className="px-3 py-2">Type</th>
                <th className="px-3 py-2">Size</th>
                <th className="px-3 py-2">Modified</th>
                <th className="px-3 py-2">Notes</th>
                <th className="w-20 px-3 py-2"></th>
              </tr>
            </thead>
            <tbody>
              {rows.map((f) => {
                const selectable = !f.missing && f.status !== "quarantined";
                return (
                  <tr
                    key={f.id}
                    className="border-b border-border-subtle last:border-0 hover:bg-soft"
                  >
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        aria-label={`Select ${f.relativePath}`}
                        checked={selected.has(f.id)}
                        disabled={!selectable}
                        onChange={() => toggleOne(f.id)}
                      />
                    </td>
                    <td className="max-w-[380px] px-3 py-2">
                      <div className="truncate font-medium text-ink" title={f.relativePath}>
                        {f.currentFilename}
                      </div>
                      <div className="truncate text-xs text-ink-muted" title={f.relativePath}>
                        {f.relativePath}
                      </div>
                    </td>
                    <td className="px-3 py-2">
                      <Pill tone={TYPE_TONES[f.fileType] ?? "neutral"}>
                        {f.fileType}
                      </Pill>
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-ink-secondary">
                      {formatBytes(f.sizeBytes)}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-ink-secondary">
                      {formatDateTime(f.modifiedAtFs)}
                    </td>
                    <td className="px-3 py-2">
                      <span className="flex flex-wrap gap-1">
                        {f.missing ? <Pill tone="warning">missing</Pill> : null}
                        {f.status === "quarantined" ? (
                          <Pill tone="rose">quarantined</Pill>
                        ) : null}
                        {f.zeroByte ? <Pill tone="warning">0 bytes</Pill> : null}
                        {f.deepScript ? (
                          <Pill tone="danger" title="Nested deeper than the game loads scripts">
                            too deep
                          </Pill>
                        ) : null}
                        {f.parseStatus && f.parseStatus !== "ok" ? (
                          <Pill
                            tone="warning"
                            title={`This package's index couldn't be read (${f.parseStatus}) — it may be corrupt.`}
                          >
                            unreadable
                          </Pill>
                        ) : null}
                      </span>
                    </td>
                    <td className="px-3 py-2 text-right">
                      {!f.missing && f.status !== "quarantined" ? (
                        <Button
                          variant="quiet"
                          onClick={() =>
                            api.revealInExplorer(f.absolutePath).catch(reportError)
                          }
                          title="Reveal in file manager"
                        >
                          Reveal
                        </Button>
                      ) : null}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </Card>
      )}

      <div className="flex items-center justify-between">
        <Button variant="quiet" disabled={page === 0} onClick={() => setPage((p) => p - 1)}>
          ← Previous
        </Button>
        <span className="text-xs text-ink-muted">Page {page + 1}</span>
        <Button variant="quiet" disabled={!hasMore} onClick={() => setPage((p) => p + 1)}>
          Next →
        </Button>
      </div>

      {quarantining ? (
        <QuarantineDialog
          fileIds={quarantining}
          reason="Set aside from the Library"
          onClose={() => setQuarantining(null)}
        />
      ) : null}
    </div>
  );
}
