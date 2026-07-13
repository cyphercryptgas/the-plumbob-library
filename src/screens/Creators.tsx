import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as api from "../lib/commands";
import { Pagination } from "../components/Pagination";
import { Card, Icon, Pill } from "../components/ui";
import type { FileRow } from "../lib/types";
import { useApp } from "../state/AppContext";

const PAGE_SIZE = 50;

const SUB_BADGE: Record<string, string> = {
  hats: "Hats",
  hair: "Hair",
  face: "Face & Sculpts",
  fullbody: "Full Body",
  tops: "Tops",
  bottoms: "Bottoms",
  shoes: "Shoes",
  accessories: "Accessories",
  skin: "Skin & Details",
  other: "CAS · Other",
};

const CATEGORY_BADGE: Record<string, string> = {
  cas: "CAS",
  buildbuy: "Build/Buy",
  animations: "Poses & Anims",
  gameplay: "Gameplay",
  scripts: "Script",
  other: "Other",
};

/** Works by a chosen creator, as tiles — the Library grid's manners. */
function CreatorWorks(props: { creatorKey: string; display: string }) {
  const { reportError } = useApp();
  const [rows, setRows] = useState<FileRow[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  const [page, setPage] = useState(0);
  const [thumbs, setThumbs] = useState<Record<number, string | null>>({});
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [toggling, setToggling] = useState(false);

  useEffect(() => {
    setPage(0);
  }, [props.creatorKey]);

  useEffect(() => {
    let alive = true;
    Promise.all([
      api.listFiles({
        creator: props.creatorKey,
        sort: "name",
        limit: PAGE_SIZE,
        offset: page * PAGE_SIZE,
      }),
      api.countFiles({ creator: props.creatorKey }),
    ])
      .then(([data, count]) => {
        if (!alive) return;
        setRows(data);
        setTotal(count);
      })
      .catch(reportError);
    return () => {
      alive = false;
    };
  }, [props.creatorKey, page, reportError]);

  useEffect(() => {
    if (rows.length === 0) return;
    const missing = rows.map((r) => r.id).filter((id) => !(id in thumbs));
    if (missing.length === 0) return;
    let alive = true;
    api
      .getThumbnails(missing)
      .then((got) => {
        if (!alive) return;
        setThumbs((prev) => {
          const next = { ...prev };
          for (const t of got) {
            next[t.fileId] = t.path ? convertFileSrc(t.path) : null;
          }
          return next;
        });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows]);

  const toggleFile = async (f: FileRow) => {
    setToggling(true);
    try {
      await api.setFilesEnabled([f.id], !f.enabled);
      setRows((prev) =>
        prev.map((r) => (r.id === f.id ? { ...r, enabled: !f.enabled } : r))
      );
    } catch (e) {
      reportError(e);
    } finally {
      setToggling(false);
    }
  };

  return (
    <Card>
      <div className="mb-3 flex items-baseline justify-between gap-3">
        <h3 className="font-display text-base font-semibold text-ink">
          {props.display}
        </h3>
        <span className="text-xs text-ink-muted">
          {total !== null ? `${total.toLocaleString()} files` : "…"}
        </span>
      </div>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
        {rows.map((f) => {
          const expanded = expandedId === f.id;
          const selectable = !f.missing && f.status !== "quarantined";
          return (
            <div key={f.id} className="min-w-0">
              <button
                type="button"
                onClick={() =>
                  setExpandedId((cur) => (cur === f.id ? null : f.id))
                }
                title={f.relativePath}
                className={`raised-pill flex aspect-square w-full items-center justify-center overflow-hidden rounded-xl border border-gold/40 bg-soft ${f.enabled ? "" : "opacity-60"}`}
              >
                {thumbs[f.id] ? (
                  <img
                    src={thumbs[f.id] as string}
                    alt=""
                    loading="lazy"
                    className="h-full w-full object-cover"
                  />
                ) : (
                  <Icon name="package" size={24} className="text-sage-deep opacity-70" />
                )}
              </button>
              <div
                className={`mt-1 truncate text-xs ${f.enabled ? "text-ink" : "text-ink-muted"}`}
                title={f.currentFilename}
              >
                {f.currentFilename}
              </div>
              <div className="flex flex-wrap gap-1">
                {f.casSubcategory && SUB_BADGE[f.casSubcategory] ? (
                  <Pill tone="sage">{SUB_BADGE[f.casSubcategory]}</Pill>
                ) : f.category && CATEGORY_BADGE[f.category] ? (
                  <Pill tone="sage">{CATEGORY_BADGE[f.category]}</Pill>
                ) : null}
                {!f.enabled && f.status === "current" ? (
                  <Pill tone="neutral">off</Pill>
                ) : null}
              </div>
              {expanded && selectable ? (
                <div className="mt-1 flex gap-1">
                  {f.fileType === "package" || f.fileType === "ts4script" ? (
                    <button
                      type="button"
                      disabled={toggling}
                      onClick={() => void toggleFile(f)}
                      className="rounded-control border border-border-subtle px-2 py-0.5 text-[11px] text-ink-secondary"
                    >
                      {f.enabled ? "Disable" : "Enable"}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    onClick={() =>
                      api.revealInExplorer(f.absolutePath).catch(reportError)
                    }
                    className="rounded-control border border-border-subtle px-2 py-0.5 text-[11px] text-ink-secondary"
                  >
                    Reveal
                  </button>
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
      {total !== null ? (
        <div className="mt-3 flex justify-center">
          <Pagination
            page={page}
            pageCount={Math.max(1, Math.ceil(total / PAGE_SIZE))}
            onPage={setPage}
          />
        </div>
      ) : null}
    </Card>
  );
}

export function Creators(props: { seed?: { key: string; n: number } }) {
  const { reportError } = useApp();
  const [roster, setRoster] = useState<api.CreatorRow[] | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<api.CreatorRow | null>(null);

  useEffect(() => {
    if (!props.seed?.key || !roster) return;
    const hit = roster.find((r) => r.key === props.seed?.key);
    if (hit) {
      setQuery("");
      setSelected(hit);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.seed?.n, roster]);

  useEffect(() => {
    let alive = true;
    api
      .creatorsOverview()
      .then((rows) => {
        if (!alive) return;
        setRoster(rows);
        setSelected((cur) => cur ?? rows[0] ?? null);
      })
      .catch(reportError);
    return () => {
      alive = false;
    };
  }, [reportError]);

  const shown = (roster ?? []).filter(
    (r) =>
      !query || r.display.toLowerCase().includes(query.trim().toLowerCase())
  );

  return (
    <div className="space-y-4">
      <Card>
        <div className="flex flex-wrap items-center gap-3">
          <div className="min-w-[220px] flex-1">
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search creators…"
              className="w-full rounded-control border border-border-subtle bg-surface px-3 py-2 text-sm"
            />
          </div>
          <span className="text-xs text-ink-muted">
            {roster
              ? `${roster.length.toLocaleString()} creators credited`
              : "Reading the credits…"}
          </span>
        </div>
        {roster && roster.length === 0 ? (
          <p className="mt-3 text-sm text-ink-secondary">
            No creators credited yet — run a Scan from the Dashboard and the
            bylines will be read from your filenames.
          </p>
        ) : (
          <div className="mt-3 flex flex-wrap gap-1.5">
            {shown.slice(0, 60).map((r) => (
              <button
                key={r.key}
                type="button"
                onClick={() => setSelected(r)}
                title={
                  r.onCurse > 0
                    ? `${r.files} files · ${r.onCurse} matched on CurseForge`
                    : `${r.files} files`
                }
                className={`rounded-control border px-2.5 py-1 text-xs transition ${
                  selected?.key === r.key
                    ? "border-transparent bg-accent font-medium text-ink"
                    : "border-border-subtle text-ink-secondary hover:border-gold/60"
                }`}
              >
                {r.display}
                <span className="ml-1.5 text-[10px] text-ink-muted">
                  {r.files.toLocaleString()}
                </span>
                {r.onCurse > 0 ? (
                  <span className="ml-1 text-[10px] text-sage-deep">
                    ✦{r.onCurse.toLocaleString()}
                  </span>
                ) : null}
              </button>
            ))}
            {shown.length > 60 ? (
              <span className="self-center text-xs text-ink-muted">
                +{(shown.length - 60).toLocaleString()} more — narrow the search
              </span>
            ) : null}
          </div>
        )}
      </Card>
      {selected ? (
        <CreatorWorks creatorKey={selected.key} display={selected.display} />
      ) : null}
    </div>
  );
}
