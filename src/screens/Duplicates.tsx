import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getThumbnails } from "../lib/commands";
import { useEffect as useThumbEffect, useState as useThumbState } from "react";
import * as api from "../lib/commands";
import { formatBytes, formatDateTime, plural, shortHash } from "../lib/format";
import type { DuplicateGroupView, SuspectedDuplicateGroup } from "../lib/types";
import { useApp } from "../state/AppContext";
import { Button, Card, EmptyState, Pill, SectionTitle } from "../components/ui";
import { QuarantineDialog } from "../components/QuarantineDialog";

interface PendingQuarantine {
  fileIds: number[];
  reason: string;
  groupId: number;
}

export function Duplicates() {
  const { libraryVersion, refreshCounts, reportError } = useApp();
  const [groups, setGroups] = useState<DuplicateGroupView[]>([]);
  const [loading, setLoading] = useState(true);
  /** Per-group chosen "keep" file (defaults to the recommendation). */
  const [keep, setKeep] = useState<Record<number, number>>({});
  const [thumbs, setThumbs] = useThumbState<Record<number, string>>({});
  useThumbEffect(() => {
    const ids = groups.flatMap((g) => g.members.map((m) => m.fileId));
    if (ids.length === 0) return;
    let alive = true;
    getThumbnails(ids.slice(0, 120))
      .then((got) => {
        if (!alive) return;
        setThumbs((prev) => {
          const next = { ...prev };
          for (const t of got) if (t.path) next[t.fileId] = convertFileSrc(t.path);
          return next;
        });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [groups]);
  const [pending, setPending] = useState<PendingQuarantine | null>(null);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    api
      .listDuplicateGroups()
      .then((data) => {
        if (!alive) return;
        setGroups(data);
        setKeep((prev) => {
          const next: Record<number, number> = {};
          for (const g of data) {
            next[g.id] =
              prev[g.id] ??
              g.recommendedFileId ??
              g.members[0]?.fileId ??
              -1;
          }
          return next;
        });
      })
      .catch(reportError)
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [libraryVersion, reportError]);

  const dismiss = useCallback(
    async (groupId: number) => {
      try {
        await api.setDuplicateGroupStatus(groupId, "dismissed");
        setGroups((gs) => gs.filter((g) => g.id !== groupId));
        await refreshCounts();
      } catch (e) {
        reportError(e);
      }
    },
    [refreshCounts, reportError]
  );

  const beginSetAside = useCallback(
    (group: DuplicateGroupView) => {
      const keepId = keep[group.id];
      const kept = group.members.find((m) => m.fileId === keepId);
      const others = group.members
        .filter((m) => m.fileId !== keepId)
        .map((m) => m.fileId);
      if (others.length === 0 || !kept) return;
      setPending({
        fileIds: others,
        groupId: group.id,
        reason: `Exact duplicate of ${kept.relativePath}`,
      });
    },
    [keep]
  );

  const totalReclaimable = groups.reduce((s, g) => s + g.reclaimableBytes, 0);

  if (!loading && groups.length === 0) {
    return (
      <EmptyState
        title="No duplicates to review ✧"
        body="Every file in your library is one of a kind right now. New scans re-check automatically."
      />
    );
  }

  return (
    <div className="space-y-4">
      <Card>
        <SectionTitle hint="Groups of byte-identical files, found by content fingerprint. Pick which copy stays; the rest are backed up and set aside — never deleted.">
          {loading
            ? "Checking…"
            : `${plural(groups.length, "group")} · ${formatBytes(totalReclaimable)} reclaimable`}
        </SectionTitle>
        <p className="text-xs text-ink-muted">
          Recommendations prefer copies linked to a known mod, in tidier
          locations, or seen in your library longer — and every one says why.
        </p>
      </Card>

      {groups.map((group) => {
        const keepId = keep[group.id];
        const settingAsideCount = group.members.length - 1;
        return (
          <Card key={group.id}>
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <Pill tone="blue">{formatBytes(group.sizeBytes ?? 0)} each</Pill>
              <Pill tone="sage">
                {formatBytes(group.reclaimableBytes)} reclaimable
              </Pill>
              <Pill tone="neutral" title={group.sha256 ?? undefined}>
                fingerprint {shortHash(group.sha256)}
              </Pill>
            </div>

            <fieldset>
              <legend className="sr-only">
                Choose which copy to keep in group {group.id}
              </legend>
              <ul className="space-y-2">
                {group.members.map((m) => (
                  <li
                    key={m.fileId}
                    className={`flex items-start gap-3 rounded-control border p-3 ${
                      keepId === m.fileId
                        ? "border-sage bg-sage-soft"
                        : "border-border-subtle bg-surface"
                    }`}
                  >
                    <input
                      type="radio"
                      name={`keep-${group.id}`}
                      className="mt-1"
                      checked={keepId === m.fileId}
                      onChange={() =>
                        setKeep((k) => ({ ...k, [group.id]: m.fileId }))
                      }
                      aria-label={`Keep ${m.relativePath}`}
                    />
                    {thumbs[m.fileId] ? (
                      <img
                        src={thumbs[m.fileId]}
                        alt=""
                        className="h-10 w-10 shrink-0 rounded-lg border border-gold/40 object-cover"
                      />
                    ) : null}
                    <div className="min-w-0 flex-1">
                      <div className="break-all text-sm font-medium text-ink">
                        {m.relativePath}
                      </div>
                      <div className="mt-0.5 text-xs text-ink-muted">
                        modified {formatDateTime(m.modifiedAtFs)}
                      </div>
                      {m.recommended && group.recommendationReason ? (
                        <div className="mt-1 text-xs text-sage-deep">
                          Recommended keep — {group.recommendationReason}
                        </div>
                      ) : null}
                    </div>
                    {keepId === m.fileId ? (
                      <Pill tone="sage">keeping</Pill>
                    ) : (
                      <Pill tone="neutral">will be set aside</Pill>
                    )}
                  </li>
                ))}
              </ul>
            </fieldset>

            <div className="mt-3 flex items-center justify-end gap-2">
              <Button
                variant="quiet"
                onClick={() => void dismiss(group.id)}
                title="Hide this group without changing any files. It won't come back on rescans."
              >
                Dismiss
              </Button>
              <Button onClick={() => beginSetAside(group)}>
                Back up & set aside {plural(settingAsideCount, "copy", "copies")}
              </Button>
            </div>
          </Card>
        );
      })}

      <SuspectedSection />

      {pending ? (
        <QuarantineDialog
          fileIds={pending.fileIds}
          reason={pending.reason}
          resolveGroupId={pending.groupId}
          onClose={() => setPending(null)}
        />
      ) : null}
    </div>
  );
}

/**
 * Lower-confidence tier: packages sharing a file name but carrying
 * different content. Displayed for hand review only — exact matches above
 * are the safe ones to act on, so no set-aside button lives here.
 */
function SuspectedSection() {
  const { libraryVersion, reportError } = useApp();
  const [groups, setGroups] = useState<SuspectedDuplicateGroup[]>([]);

  useEffect(() => {
    let alive = true;
    api
      .listSuspectedDuplicates()
      .then((data) => {
        if (alive) setGroups(data);
      })
      .catch(reportError);
    return () => {
      alive = false;
    };
  }, [libraryVersion, reportError]);

  if (groups.length === 0) return null;

  return (
    <div className="space-y-3 pt-2">
      <SectionTitle hint="Same file name, different content — probably versions of the same thing. Lower confidence than the fingerprint matches above, so review these by hand.">
        Suspected duplicates
      </SectionTitle>
      {groups.map((g) => (
        <Card key={g.fileName}>
          <div className="mb-2 flex items-center gap-2">
            <span className="text-sm font-medium text-ink">{g.fileName}</span>
            <Pill tone="warning">different content</Pill>
          </div>
          <ul className="space-y-2">
            {g.members.map((m) => (
              <li
                key={m.fileId}
                className="flex flex-wrap items-center justify-between gap-2 rounded-control border border-border-subtle px-3 py-2"
              >
                <span
                  className="min-w-0 flex-1 truncate text-sm text-ink-secondary"
                  title={m.relativePath}
                >
                  {m.relativePath}
                </span>
                <span className="text-xs text-ink-muted">
                  {formatBytes(m.sizeBytes)}
                </span>
                <Button
                  variant="quiet"
                  onClick={() =>
                    api.revealInExplorer(m.absolutePath).catch(reportError)
                  }
                  title="Reveal in file manager"
                >
                  Reveal
                </Button>
              </li>
            ))}
          </ul>
        </Card>
      ))}
    </div>
  );
}
