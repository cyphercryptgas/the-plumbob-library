import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { asMessage } from "../lib/tauri";
import { AFFILIATION_DISCLAIMER } from "../lib/product";
import type { AppSettings } from "../lib/types";
import { useApp } from "../state/AppContext";
import {
  Banner,
  Button,
  Card,
  Field,
  Pill,
  SectionTitle,
  TextInput,
  Toggle,
} from "../components/ui";

function FolderPicker(props: {
  value: string | null;
  placeholder: string;
  title: string;
  onChange: (path: string | null) => void;
  clearable?: boolean;
}) {
  const pick = async () => {
    const picked = await open({ directory: true, title: props.title });
    if (typeof picked === "string") props.onChange(picked);
  };
  return (
    <div className="flex items-center gap-2">
      <div
        className="min-w-0 flex-1 truncate rounded-control border border-border-subtle bg-soft px-3 py-2 text-sm text-ink"
        title={props.value ?? undefined}
      >
        {props.value ?? (
          <span className="text-ink-muted">{props.placeholder}</span>
        )}
      </div>
      <Button variant="soft" onClick={() => void pick()}>
        Browse…
      </Button>
      {props.clearable && props.value ? (
        <Button variant="quiet" onClick={() => props.onChange(null)}>
          Use default
        </Button>
      ) : null}
    </div>
  );
}

export function Settings() {
  const { settings, info, saveSettings, refreshCounts } = useApp();
  const [draft, setDraft] = useState<AppSettings | null>(settings);
  const [newExclusion, setNewExclusion] = useState("");
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  const dirty = useMemo(
    () => JSON.stringify(draft) !== JSON.stringify(settings),
    [draft, settings]
  );

  const update = useCallback(
    <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
      setSaved(false);
      setDraft((d) => (d ? { ...d, [key]: value } : d));
    },
    []
  );

  const addExclusion = useCallback(() => {
    const trimmed = newExclusion.trim().replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
    if (!trimmed || !draft) return;
    if (draft.scanExcluded.some((e) => e.toLowerCase() === trimmed.toLowerCase()))
      return;
    update("scanExcluded", [...draft.scanExcluded, trimmed]);
    setNewExclusion("");
  }, [newExclusion, draft, update]);

  const save = useCallback(async () => {
    if (!draft) return;
    setError(null);
    try {
      const ok = await saveSettings(draft);
      if (ok) {
        setSaved(true);
        await refreshCounts();
      }
    } catch (e) {
      setError(asMessage(e));
    }
  }, [draft, saveSettings, refreshCounts]);

  if (!draft) return null;

  return (
    <div className="space-y-5">
      <Card>
        <SectionTitle hint="Where this app looks and where it keeps its safety copies.">
          Folders
        </SectionTitle>
        <Field
          label="Mods folder"
          hint="Your Sims 4 Mods folder. Changing it points the whole library at a different collection — run a scan afterwards."
        >
          <FolderPicker
            value={draft.modsFolder}
            placeholder="Not set"
            title="Choose your Sims 4 Mods folder"
            onChange={(p) => update("modsFolder", p)}
          />
        </Field>
        <Field
          label="Backup folder"
          hint="Automatic pre-change snapshots are stored here. Default: the app's data folder."
        >
          <FolderPicker
            value={draft.backupFolder}
            placeholder="App data folder (default)"
            title="Choose a backup folder"
            onChange={(p) => update("backupFolder", p)}
            clearable
          />
        </Field>
        <Field
          label="Quarantine folder"
          hint="Files you set aside wait here, fully restorable. Default: the app's data folder."
        >
          <FolderPicker
            value={draft.quarantineFolder}
            placeholder="App data folder (default)"
            title="Choose a quarantine folder"
            onChange={(p) => update("quarantineFolder", p)}
            clearable
          />
        </Field>
        <p className="mt-1 text-xs text-ink-muted">
          Backup and quarantine folders can't live inside the Mods folder —
          the app will politely refuse.
        </p>
      </Card>

      <Card>
        <SectionTitle hint="Folders the scanner skips entirely (relative to the Mods folder).">
          Scan exclusions
        </SectionTitle>
        <div className="flex flex-wrap gap-2">
          {draft.scanExcluded.length === 0 ? (
            <p className="text-sm text-ink-muted">Nothing excluded.</p>
          ) : (
            draft.scanExcluded.map((prefix) => (
              <span
                key={prefix}
                className="inline-flex items-center gap-1.5 rounded-full bg-soft px-3 py-1 text-xs text-ink-secondary"
              >
                {prefix}
                <button
                  type="button"
                  aria-label={`Remove exclusion ${prefix}`}
                  className="font-semibold text-ink-muted hover:text-danger"
                  onClick={() =>
                    update(
                      "scanExcluded",
                      draft.scanExcluded.filter((e) => e !== prefix)
                    )
                  }
                >
                  ✕
                </button>
              </span>
            ))
          )}
        </div>
        <div className="mt-3 flex items-center gap-2">
          <div className="flex-1">
            <TextInput
              value={newExclusion}
              onChange={setNewExclusion}
              placeholder="e.g. Disabled or WIP/Drafts"
              ariaLabel="New exclusion prefix"
            />
          </div>
          <Button variant="soft" onClick={addExclusion} disabled={!newExclusion.trim()}>
            Add
          </Button>
        </div>
        <p className="mt-2 text-xs text-ink-muted">
          Excluded files are never marked "missing" — skipped isn't the same
          as gone.
        </p>
      </Card>

      <Card>
        <SectionTitle>Behavior</SectionTitle>
        <Toggle
          checked={draft.stopOnError}
          onChange={(v) => update("stopOnError", v)}
          label="Stop multi-file operations on the first problem"
          hint="The safe default. Turn off to let batches continue past individual failures."
        />
        <Toggle
          checked={draft.reducedMotion}
          onChange={(v) => update("reducedMotion", v)}
          label="Reduce motion"
          hint="Also follows your system preference automatically."
        />
        <Field
          label="Script depth limit"
          hint="Scripts nested deeper than this (folders below the Mods root) are flagged, since the game's default Resource.cfg won't load them."
        >
          <input
            type="number"
            min={0}
            max={10}
            value={draft.scriptDepthLimit}
            aria-label="Script depth limit"
            onChange={(e) => {
              const v = Number(e.target.value);
              if (Number.isFinite(v))
                update("scriptDepthLimit", Math.max(0, Math.min(10, Math.round(v))));
            }}
            className="w-24 rounded-control border border-border-subtle bg-surface px-3 py-2 text-sm text-ink"
          />
        </Field>
      </Card>

      <Card>
        <SectionTitle>About</SectionTitle>
        {info ? (
          <dl className="space-y-1 text-sm">
            <div className="flex gap-2">
              <dt className="w-24 shrink-0 text-ink-muted">Version</dt>
              <dd className="text-ink">{info.version}</dd>
            </div>
            <div className="flex gap-2">
              <dt className="w-24 shrink-0 text-ink-muted">Data folder</dt>
              <dd className="break-all text-ink-secondary">{info.dataDir}</dd>
            </div>
            <div className="flex gap-2">
              <dt className="w-24 shrink-0 text-ink-muted">Database</dt>
              <dd className="break-all text-ink-secondary">{info.dbPath}</dd>
            </div>
          </dl>
        ) : null}
        <p className="mt-3 text-[11px] leading-relaxed text-ink-muted">
          {AFFILIATION_DISCLAIMER}
        </p>
      </Card>

      {error ? (
        <Banner tone="danger" onDismiss={() => setError(null)}>
          {error}
        </Banner>
      ) : null}

      <div className="flex items-center gap-3">
        <Button onClick={() => void save()} disabled={!dirty}>
          Save settings
        </Button>
        {saved && !dirty ? <Pill tone="sage">Saved ✓</Pill> : null}
        {dirty ? <Pill tone="neutral">Unsaved changes</Pill> : null}
      </div>
    </div>
  );
}
