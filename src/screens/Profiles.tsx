import { useEffect, useState } from "react";
import {
  activeProfile as fetchActive,
  createProfile,
  deleteProfile,
  listProfiles,
  renameProfile,
  setActiveProfile,
} from "../lib/commands";
import { useApp } from "../state/AppContext";
import type { ProfileView } from "../lib/types";
import { Button, Card, Icon, Pill, SectionTitle } from "../components/ui";

function shortDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

export function Profiles() {
  const { reportError, setActiveProfile: pushActive } = useApp();
  const [profiles, setProfiles] = useState<ProfileView[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [renaming, setRenaming] = useState<{ id: number; name: string } | null>(
    null
  );
  const [confirmingDelete, setConfirmingDelete] = useState<number | null>(null);

  const refresh = async () => {
    try {
      setProfiles(await listProfiles());
      pushActive(await fetchActive());
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

  const run = async (work: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await work();
      await refresh();
    } catch (e) {
      reportError(e);
    } finally {
      setBusy(false);
      setRenaming(null);
      setConfirmingDelete(null);
    }
  };

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <Card finials>
        <SectionTitle>Who's holding the save?</SectionTitle>
        <p className="text-sm leading-relaxed text-ink-secondary">
          A profile names the person — or the setup — this library belongs to.
          The active profile is who the app greets on the Dashboard.
        </p>
        <p className="mt-2 text-sm leading-relaxed text-ink-secondary">
          <span
            aria-hidden="true"
            className="mr-1 text-[11px] text-gold [text-shadow:0_0_8px_rgba(201,164,92,0.9)]"
          >
            ✦
          </span>
          Coming next: each profile will remember its own set of enabled and
          disabled mods, and switching profiles will arrange your library to
          match — journaled and reversible, like everything else here. This
          screen says so honestly instead of pretending it's built.
        </p>
        <div className="mt-5 flex flex-wrap items-center gap-3">
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && draft.trim())
                void run(() => createProfile(draft.trim())).then(() =>
                  setDraft("")
                );
            }}
            placeholder="e.g. Michael"
            aria-label="New profile name"
            className="w-full max-w-xs rounded-control border border-border-subtle bg-surface px-3 py-2 text-sm text-ink placeholder:text-ink-muted"
          />
          <Button
            disabled={busy || !draft.trim()}
            onClick={() =>
              void run(() => createProfile(draft.trim())).then(() =>
                setDraft("")
              )
            }
          >
            Create profile
          </Button>
          {profiles.length === 0 && loaded ? (
            <span className="text-xs text-ink-muted">
              Your first profile becomes active automatically.
            </span>
          ) : null}
        </div>
      </Card>

      {profiles.length > 0 ? (
        <Card finials>
          <SectionTitle>Profiles</SectionTitle>
          <div>
            {profiles.map((p) => (
              <div
                key={p.id}
                className="flex items-center gap-3 border-b border-gold/25 px-2 py-3 last:border-0"
              >
                <span className="icon-chip flex h-11 w-11 shrink-0 items-center justify-center rounded-xl font-display text-[18px] font-bold">
                  {p.name.charAt(0).toUpperCase()}
                </span>
                <span className="min-w-0 flex-1">
                  {renaming?.id === p.id ? (
                    <input
                      autoFocus
                      value={renaming.name}
                      onChange={(e) =>
                        setRenaming({ id: p.id, name: e.target.value })
                      }
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && renaming.name.trim())
                          void run(() =>
                            renameProfile(p.id, renaming.name.trim())
                          );
                        if (e.key === "Escape") setRenaming(null);
                      }}
                      aria-label={`Rename ${p.name}`}
                      className="w-full max-w-xs rounded-control border border-gold/60 bg-surface px-2 py-1 text-sm text-ink"
                    />
                  ) : (
                    <>
                      <span className="block truncate text-sm font-medium text-ink">
                        {p.name}
                      </span>
                      <span className="block text-xs text-ink-muted">
                        since {shortDate(p.createdAt)}
                      </span>
                    </>
                  )}
                </span>
                {p.isActive ? (
                  <Pill tone="sage">Active ✦</Pill>
                ) : (
                  <Button
                    variant="quiet"
                    disabled={busy}
                    onClick={() => void run(() => setActiveProfile(p.id))}
                  >
                    Make active
                  </Button>
                )}
                <button
                  type="button"
                  disabled={busy}
                  title={`Rename ${p.name}`}
                  aria-label={`Rename ${p.name}`}
                  onClick={() =>
                    setRenaming(
                      renaming?.id === p.id ? null : { id: p.id, name: p.name }
                    )
                  }
                  className="rounded-control p-1.5 text-ink-muted transition hover:bg-gold/10 hover:text-ink"
                >
                  <Icon name="settings" size={15} />
                </button>
                {confirmingDelete === p.id ? (
                  <span className="flex items-center gap-2">
                    <Button
                      variant="quiet"
                      disabled={busy}
                      onClick={() => void run(() => deleteProfile(p.id))}
                    >
                      Delete
                    </Button>
                    <Button
                      variant="quiet"
                      onClick={() => setConfirmingDelete(null)}
                    >
                      Keep
                    </Button>
                  </span>
                ) : (
                  <button
                    type="button"
                    disabled={busy}
                    title={`Delete ${p.name}`}
                    aria-label={`Delete ${p.name}`}
                    onClick={() => setConfirmingDelete(p.id)}
                    className="rounded-control p-1.5 text-ink-muted transition hover:bg-gold/10 hover:text-warning"
                  >
                    <Icon name="alert" size={15} />
                  </button>
                )}
              </div>
            ))}
          </div>
        </Card>
      ) : null}
    </div>
  );
}
