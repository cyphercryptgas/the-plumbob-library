import { useCallback, useEffect, useState } from "react";
import * as api from "../lib/commands";
import { formatDateTime, plural, shortHash } from "../lib/format";
import type { OperationStepView, OperationView } from "../lib/types";
import { useApp } from "../state/AppContext";
import { Card, EmptyState, Icon, Pill } from "../components/ui";
import type { IconName } from "../components/ui";
import { convertFileSrc } from "@tauri-apps/api/core";

const basename = (p: string) => p.split(/[\\/]/).pop() ?? p;

const ACTION_VERB: Record<string, string> = {
  rename: "Retitled",
  copy: "Backed up",
  move: "Moved",
  restore: "Restored",
  delete: "Removed",
};

const TYPE_ICON: Record<string, IconName> = {
  rename: "tag",
  quarantine: "duplicates",
  restore_quarantined: "library",
  snapshot: "backups",
  restore_from_snapshot: "backups",
};

const STATUS_TONE: Record<string, "sage" | "warning" | "danger" | "blue"> = {
  completed: "sage",
  partial: "warning",
  failed: "danger",
  running: "blue",
};

const TYPE_LABEL: Record<string, string> = {
  rename: "Retitled files",
  quarantine: "Set files aside",
  restore_quarantined: "Restore from quarantine",
  snapshot: "Backup snapshot",
  restore_from_snapshot: "Restore from backup",
};

export function Activity() {
  const { libraryVersion, reportError } = useApp();
  const [operations, setOperations] = useState<OperationView[]>([]);
  const [loading, setLoading] = useState(true);
  const [openId, setOpenId] = useState<number | null>(null);
  const [steps, setSteps] = useState<Record<number, OperationStepView[]>>({});
  const [thumbs, setThumbs] = useState<Record<number, string>>({});

  useEffect(() => {
    let alive = true;
    setLoading(true);
    api
      .listOperations(200)
      .then((data) => alive && setOperations(data))
      .catch(reportError)
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [libraryVersion, reportError]);

  const toggle = useCallback(
    async (op: OperationView) => {
      if (openId === op.id) {
        setOpenId(null);
        return;
      }
      setOpenId(op.id);
      if (!steps[op.id]) {
        try {
          const list = await api.listOperationSteps(op.id);
          setSteps((s) => ({ ...s, [op.id]: list }));
          const ids = Array.from(
            new Set(
              list
                .map((s) => s.fileId)
                .filter((v): v is number => typeof v === "number")
            )
          ).slice(0, 60);
          if (ids.length > 0) {
            const got = await api.getThumbnails(ids);
            setThumbs((t) => {
              const next = { ...t };
              for (const g of got) {
                if (g.path) next[g.fileId] = convertFileSrc(g.path);
              }
              return next;
            });
          }
        } catch (e) {
          reportError(e);
        }
      }
    },
    [openId, steps, reportError]
  );

  if (!loading && operations.length === 0) {
    return (
      <EmptyState
        title="No activity yet"
        body="Every operation this app performs — backups, quarantines, restores — is journaled here step by step, with the content fingerprint that verified each move."
      />
    );
  }

  return (
    <div className="space-y-3">
      <Card>
        <p className="max-w-xl text-sm leading-relaxed text-ink-secondary">
          The complete journal. Each entry lists exactly what moved where, in
          order, with the fingerprint that verified it — including the steps
          that failed and why.
        </p>
      </Card>

      {operations.map((op) => {
        const isOpen = openId === op.id;
        const list = steps[op.id];
        return (
          <Card key={op.id}>
            <button
              type="button"
              className="flex w-full flex-wrap items-center justify-between gap-3 text-left"
              onClick={() => void toggle(op)}
              aria-expanded={isOpen}
            >
              <span className="icon-chip flex h-9 w-9 shrink-0 items-center justify-center rounded-lg">
                <Icon
                  name={TYPE_ICON[op.operationType] ?? "activity"}
                  size={16}
                />
              </span>
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium text-ink">
                  {TYPE_LABEL[op.operationType] ?? op.operationType}
                </div>
                <div className="mt-0.5 text-xs text-ink-muted">
                  {formatDateTime(op.createdAt)}
                  {op.summary ? ` · ${op.summary}` : ""}
                  {op.backupId ? ` · backup #${op.backupId}` : ""}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Pill tone={STATUS_TONE[op.status] ?? "neutral"}>{op.status}</Pill>
                <span className="text-xs text-ink-muted">{isOpen ? "▴" : "▾"}</span>
              </div>
            </button>

            {isOpen ? (
              <div className="mt-3 border-t border-border-subtle pt-3">
                {!list ? (
                  <p className="text-sm text-ink-muted">Loading steps…</p>
                ) : list.length === 0 ? (
                  <p className="text-sm text-ink-muted">
                    This operation recorded no individual steps.
                  </p>
                ) : (
                  <ol className="space-y-2">
                    {list.map((step) => (
                      <li
                        key={step.stepOrder}
                        className="rounded-control bg-soft px-3 py-2 text-xs"
                      >
                        <div className="flex flex-wrap items-center gap-2">
                          {step.fileId != null && thumbs[step.fileId] ? (
                            <img
                              src={thumbs[step.fileId]}
                              alt=""
                              className="h-8 w-8 shrink-0 rounded-md border border-gold/40 object-cover"
                            />
                          ) : (
                            <span className="icon-chip flex h-8 w-8 shrink-0 items-center justify-center rounded-md">
                              <Icon name="package" size={14} />
                            </span>
                          )}
                          <span className="font-semibold text-ink">
                            {basename(step.sourcePath)}
                          </span>
                          <span className="text-ink-secondary">
                            {ACTION_VERB[step.action] ?? step.action}
                          </span>
                          <Pill
                            tone={step.status === "succeeded" ? "sage" : "danger"}
                          >
                            {step.status}
                          </Pill>
                          {step.expectedHash ? (
                            <span
                              className="text-ink-muted"
                              title={step.expectedHash}
                            >
                              verified {shortHash(step.expectedHash)}
                            </span>
                          ) : null}
                        </div>
                        <div className="mt-1 break-all text-[11px] text-ink-muted">
                          {step.sourcePath}
                          {step.destinationPath ? (
                            <>
                              {" "}
                              <span className="text-ink-muted">→</span>{" "}
                              {step.destinationPath}
                            </>
                          ) : null}
                        </div>
                        {step.errorMessage ? (
                          <div className="mt-1 text-danger">{step.errorMessage}</div>
                        ) : null}
                      </li>
                    ))}
                  </ol>
                )}
              </div>
            ) : null}
          </Card>
        );
      })}

      {!loading ? (
        <p className="text-center text-xs text-ink-muted">
          Showing the most recent {plural(operations.length, "operation")}
        </p>
      ) : null}
    </div>
  );
}
