import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getThumbnails } from "../lib/commands";
import { useEffect as useThumbEffect, useState as useThumbState } from "react";
import * as api from "../lib/commands";
import { plural } from "../lib/format";
import type { ConflictGroup } from "../lib/types";
import { useApp } from "../state/AppContext";
import { Button, Card, EmptyState, Pill, SectionTitle } from "../components/ui";

/**
 * Resource conflicts: two or more packages carrying a resource with the same
 * type-group-instance key. Only one copy can win in game. Detection reads
 * package indexes only — honest about what that can and cannot see.
 */
export function Conflicts() {
  const { libraryVersion, reportError, refreshAll } = useApp();
  const [groups, setGroups] = useState<ConflictGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [showFine, setShowFine] = useState(false);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    api
      .listConflicts()
      .then((data) => {
        if (alive) setGroups(data);
      })
      .catch(reportError)
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [libraryVersion, reportError]);

  const needsLook = groups.filter(
    (g) => g.severity === "gameplay" && !g.likelyIntentional
  );
  const probablyFine = groups.filter(
    (g) => g.severity !== "gameplay" || g.likelyIntentional
  );

  return (
    <div className="space-y-4">
      <Card>
        <SectionTitle hint="Two packages carrying the same resource compete for one slot — only one wins in game.">
          What counts as a conflict
        </SectionTitle>
        <p className="text-sm leading-relaxed text-ink-secondary">
          Load order here follows file names (A→Z), the way the game is
          commonly understood to load packages — the last copy listed is the
          presumptive winner. A few honest limits: this reads package{" "}
          <span className="font-medium text-ink">indexes</span>, so it can't
          tell whether two copies of a resource are actually different inside
          (byte-identical <em>files</em> are handled by Duplicates instead),
          script mods (.ts4script) can't be analyzed this way at all, and a key
          stamped into a dozen-plus packages is treated as tool boilerplate
          rather than a collision.
        </p>
      </Card>

      {loading ? (
        <p className="text-sm text-ink-muted">Checking package indexes…</p>
      ) : groups.length === 0 ? (
        <EmptyState
          title="No conflicts detected"
          body="No two readable packages in your library claim the same resource. Rescan after adding mods to keep this current."
        />
      ) : (
        <>
          {needsLook.length > 0 ? (
            <SectionTitle hint="Gameplay-affecting overlaps between unrelated mods.">
              Needs a look
            </SectionTitle>
          ) : (
            <Card>
              <p className="text-sm text-ink-secondary">
                Nothing looks risky: every overlap found is either
                appearance-only or lives inside one mod's own folder.
              </p>
            </Card>
          )}
          {needsLook.map((g, i) => (
            <ConflictCard key={`look-${i}`} group={g} onResolved={() => void refreshAll()} />
          ))}

          {probablyFine.length > 0 ? (
            <div className="pt-2">
              <Button variant="quiet" onClick={() => setShowFine((v) => !v)}>
                {showFine ? "Hide" : "Show"}{" "}
                {plural(probablyFine.length, "overlap")} that{" "}
                {probablyFine.length === 1 ? "looks" : "look"} intentional or
                appearance-only {showFine ? "▴" : "▾"}
              </Button>
              {showFine
                ? probablyFine.map((g, i) => (
                    <div key={`fine-${i}`} className="mt-3">
                      <ConflictCard group={g} onResolved={() => void refreshAll()} />
                    </div>
                  ))
                : null}
            </div>
          ) : null}
        </>
      )}
    </div>
  );
}

function ConflictCard({ group, onResolved }: { group: ConflictGroup; onResolved: () => void }) {
  const [thumbs, setThumbs] = useThumbState<Record<number, string>>({});
  useThumbEffect(() => {
    const ids = group.members.map((m) => m.fileId);
    if (ids.length === 0) return;
    let alive = true;
    getThumbnails(ids)
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
  }, [group]);

  const { reportError } = useApp();
  const [settingAside, setSettingAside] = useState<number | null>(null);
  const extraKeys = group.sharedKeyCount - group.sampleKeys.length;

  return (
    <Card>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        {group.severity === "gameplay" ? (
          <Pill tone="rose">gameplay</Pill>
        ) : (
          <Pill tone="neutral">appearance only</Pill>
        )}
        {group.likelyIntentional ? (
          <Pill tone="sage" title="Members share a folder or mod — overrides inside one mod are usually by design.">
            likely intentional
          </Pill>
        ) : null}
        <Pill tone="neutral">
          {plural(group.sharedKeyCount, "shared resource")}
        </Pill>
      </div>

      <ul className="space-y-2">
        {group.members.map((m, i) => {
          const winner = i === group.members.length - 1;
          return (
            <li
              key={m.fileId}
              className="flex flex-wrap items-center justify-between gap-2 rounded-control border border-border-subtle px-3 py-2"
            >
              {thumbs[m.fileId] ? (
                <img
                  src={thumbs[m.fileId]}
                  alt=""
                  className="h-9 w-9 shrink-0 rounded-lg border border-gold/40 object-cover"
                />
              ) : null}
              <span
                className="min-w-0 flex-1 truncate text-sm text-ink"
                title={m.relativePath}
              >
                {m.relativePath}
              </span>
              {winner ? (
                <Pill tone="sage" title="Loads last alphabetically — its copy of the shared resources is the one the game presumably uses.">
                  loads last · presumptive winner
                </Pill>
              ) : (
                <Pill tone="neutral">overridden</Pill>
              )}
              {!winner ? (
                <Button
                  variant="soft"
                  disabled={settingAside === m.fileId}
                  title="Quarantine this shadowed copy — the winner keeps loading; fully reversible from Quarantine."
                  onClick={() => {
                    if (
                      !window.confirm(
                        `Set aside ${m.relativePath}?\n\nThe winning copy keeps loading; this one moves to Quarantine (reversible).`
                      )
                    )
                      return;
                    setSettingAside(m.fileId);
                    api
                      .executeQuarantine([m.fileId], "Shadowed duplicate — conflict cleanup")
                      .then(() => onResolved())
                      .catch(reportError)
                      .finally(() => setSettingAside(null));
                  }}
                >
                  {settingAside === m.fileId ? "Setting aside…" : "Set aside"}
                </Button>
              ) : null}
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
          );
        })}
      </ul>

      <div className="mt-3 flex flex-wrap gap-1.5">
        {group.sampleKeys.map((k) => (
          <span
            key={k.tgi}
            title={k.tgi}
            className={`rounded-control border border-border-subtle px-2 py-0.5 font-mono text-[11px] ${
              k.presentationOnly ? "text-ink-muted" : "text-ink-secondary"
            }`}
          >
            {k.typeName ?? `0x${k.typeId.toString(16).toUpperCase().padStart(8, "0")}`}
          </span>
        ))}
        {extraKeys > 0 ? (
          <span className="px-1 py-0.5 text-[11px] text-ink-muted">
            +{extraKeys} more
          </span>
        ) : null}
      </div>
    </Card>
  );
}
