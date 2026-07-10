import { useCallback, useEffect, useState } from "react";
import * as api from "../lib/commands";
import { asMessage } from "../lib/tauri";
import { formatBytes, plural } from "../lib/format";
import type { QuarantineOutcomeDto, QuarantinePreview } from "../lib/types";
import { useApp } from "../state/AppContext";
import { Banner, Button, Modal, Pill } from "./ui";

/**
 * The one quarantine flow, shared by Library and the Duplicate Center:
 * preview (read-only) → confirm → automatic backup → hash-verified moves →
 * honest outcome. Files are set aside, never deleted.
 */
export function QuarantineDialog(props: {
  fileIds: number[];
  reason: string;
  resolveGroupId?: number;
  onClose: () => void;
}) {
  const { isGameRunning } = useApp();
  const [preview, setPreview] = useState<QuarantinePreview | null>(null);
  const [outcome, setOutcome] = useState<QuarantineOutcomeDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .previewQuarantine(props.fileIds)
      .then((p) => alive && setPreview(p))
      .catch((e) => alive && setError(asMessage(e)));
    return () => {
      alive = false;
    };
  }, [props.fileIds]);

  const run = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await api.executeQuarantine(
        props.fileIds,
        props.reason,
        props.resolveGroupId
      );
      setOutcome(result);
    } catch (e) {
      setError(asMessage(e));
    } finally {
      setBusy(false);
    }
  }, [props.fileIds, props.reason, props.resolveGroupId]);

  const blocked =
    !preview || preview.filesMissingOnDisk > 0 || isGameRunning || busy;

  return (
    <Modal
      title={outcome ? "All set aside" : "Set files aside?"}
      onClose={props.onClose}
      wide
    >
      {outcome ? (
        <div className="space-y-3">
          <Banner tone="success">
            {plural(outcome.completed, "file")} moved to quarantine —{" "}
            {formatBytes(outcome.reclaimedBytes)} reclaimed from your Mods
            folder. A verified backup was taken first (backup #
            {outcome.backupId}), and everything is restorable from the
            Quarantine screen.
          </Banner>
          {outcome.failed.length > 0 ? (
            <Banner tone="warning">
              {plural(outcome.failed.length, "file")} couldn't be moved
              {outcome.haltedEarly
                ? " — the rest of the plan was halted, as your settings ask"
                : ""}
              :
              <ul className="mt-1 list-inside list-disc">
                {outcome.failed.map((f) => (
                  <li key={f.path} className="break-all">
                    {f.path}: {f.message}
                  </li>
                ))}
              </ul>
            </Banner>
          ) : null}
          <div className="flex justify-end">
            <Button onClick={props.onClose}>Done</Button>
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          <p className="text-sm leading-relaxed text-ink-secondary">
            Setting aside isn't deleting: each file is backed up, moved to the
            quarantine folder with its content verified by fingerprint, and
            can be restored to its exact original spot any time.
          </p>

          {isGameRunning ? (
            <Banner tone="warning">
              The Sims 4 is running. Close the game first — this app never
              moves files the game might be holding open.
            </Banner>
          ) : null}

          {preview ? (
            <>
              <div className="flex flex-wrap items-center gap-2">
                <Pill tone="sage">{plural(preview.files.length, "file")}</Pill>
                <Pill tone="blue">{formatBytes(preview.totalBytes)}</Pill>
                {preview.filesWithoutHash > 0 ? (
                  <Pill
                    tone="neutral"
                    title="These will still be moved and verified; they just lack a stored fingerprint to pre-check against."
                  >
                    {preview.filesWithoutHash} without stored fingerprint
                  </Pill>
                ) : null}
                {preview.filesMissingOnDisk > 0 ? (
                  <Pill tone="danger">
                    {preview.filesMissingOnDisk} missing on disk — re-scan
                    first
                  </Pill>
                ) : null}
              </div>
              <ul className="max-h-56 space-y-1 overflow-y-auto rounded-control bg-soft p-3 text-xs text-ink-secondary">
                {preview.files.map((f) => (
                  <li key={f.id} className="break-all">
                    {f.relativePath}{" "}
                    <span className="text-ink-muted">
                      · {formatBytes(f.sizeBytes)}
                    </span>
                  </li>
                ))}
              </ul>
            </>
          ) : !error ? (
            <p className="text-sm text-ink-muted">Preparing preview…</p>
          ) : null}

          {error ? (
            <Banner tone="danger" onDismiss={() => setError(null)}>
              {error}
            </Banner>
          ) : null}

          <div className="flex items-center justify-end gap-2">
            <Button variant="quiet" onClick={props.onClose} disabled={busy}>
              Cancel
            </Button>
            <Button onClick={() => void run()} disabled={blocked}>
              {busy ? "Backing up & moving…" : "Back up & set aside"}
            </Button>
          </div>
        </div>
      )}
    </Modal>
  );
}
