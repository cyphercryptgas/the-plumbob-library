import { useEffect, useRef, useState } from "react";
import {
  onTroubleshootProgress,
  type TroubleshootProgress,
} from "../lib/events";
import {
  troubleshootAbort,
  troubleshootReconcile,
  troubleshootStart,
  troubleshootVerdict,
} from "../lib/commands";
import { useApp } from "../state/AppContext";
import type { Route } from "../components/Sidebar";
import type {
  TroubleshootReconcileReport,
  TroubleshootSession,
} from "../lib/types";
import {
  Banner,
  Button,
  Card,
  Field,
  Icon,
  Pill,
  SectionTitle,
  type IconName,
} from "../components/ui";

const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

/** Tests remaining if every verdict halves the pool, plus the confirmation. */
function estimatedTests(session: TroubleshootSession): number {
  const pool = Math.max(session.poolSize, 1);
  const halvings = Math.ceil(Math.log2(pool));
  return session.phase === "confirming" ? 1 : halvings + 1;
}

function StepDots(props: { done: number; remaining: number }) {
  const dots = [
    ...Array.from({ length: props.done }, () => true),
    ...Array.from({ length: Math.min(props.remaining, 14) }, () => false),
  ];
  return (
    <span className="inline-flex items-center gap-1.5" aria-hidden="true">
      {dots.map((filled, i) =>
        filled ? (
          <span
            key={i}
            className="h-2 w-2 rounded-full bg-gold shadow-[0_0_6px_rgba(201,164,92,0.8)]"
          />
        ) : (
          <span key={i} className="h-2 w-2 rounded-full border border-gold/60" />
        )
      )}
    </span>
  );
}

function VerdictTile(props: {
  icon: IconName;
  title: string;
  sub: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={props.onClick}
      disabled={props.disabled}
      className="gold-edge-card flex flex-1 flex-col items-center gap-2 rounded-card px-4 py-5 text-center transition-all hover:-translate-y-0.5 hover:shadow-[0_0_0_1.4px_rgba(210,170,92,0.9),0_0_22px_rgba(210,170,92,0.5)] disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:translate-y-0 disabled:hover:shadow-none"
    >
      <span className="icon-chip flex h-12 w-12 items-center justify-center rounded-xl">
        <Icon name={props.icon} size={21} />
      </span>
      <span className="font-display text-[15px] font-bold text-ink">
        {props.title}
      </span>
      <span className="text-xs leading-snug text-ink-muted">{props.sub}</span>
    </button>
  );
}

export function Troubleshoot(props: { onNavigate: (route: Route) => void }) {
  const {
    troubleshoot: contextSession,
    setTroubleshoot,
    isGameRunning,
    refreshCounts,
    reportError,
  } = useApp();

  const [session, setSession] = useState<TroubleshootSession | null>(
    contextSession
  );
  const [finished, setFinished] = useState<TroubleshootSession | null>(null);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmingAbort, setConfirmingAbort] = useState(false);
  const [reconcile, setReconcile] =
    useState<TroubleshootReconcileReport | null>(null);
  const [progress, setProgress] = useState<TroubleshootProgress | null>(null);

  useEffect(() => onTroubleshootProgress(setProgress), []);
  const reconciledFor = useRef<number | null>(null);

  // Keep in step with the app-wide session (badge, other screens).
  useEffect(() => {
    setSession(contextSession);
  }, [contextSession]);

  // Heal rows from disk truth whenever the wizard opens onto a live session.
  useEffect(() => {
    if (!session || reconciledFor.current === session.id) return;
    reconciledFor.current = session.id;
    troubleshootReconcile(session.id)
      .then(setReconcile)
      .catch((e) => reportError(e));
  }, [session, reportError]);

  const applyResult = async (view: TroubleshootSession) => {
    if (view.status === "active") {
      setSession(view);
      setTroubleshoot(view);
    } else {
      setFinished(view);
      setSession(null);
      setTroubleshoot(null);
      await refreshCounts();
    }
  };

  const run = async (work: () => Promise<TroubleshootSession>) => {
    setBusy(true);
    try {
      await applyResult(await work());
    } catch (e) {
      reportError(e);
    } finally {
      setBusy(false);
      setConfirmingAbort(false);
      setProgress(null);
    }
  };

  const verdictLocked = busy || isGameRunning;

  // ------------------------------------------------------------- finished
  if (finished) {
    const outcome = finished.outcome;
    return (
      <div className="mx-auto max-w-2xl space-y-6">
        <Card finials>
          {outcome === "culprit_confirmed" && finished.candidate ? (
            <div className="space-y-4 text-center">
              <span className="icon-chip mx-auto flex h-14 w-14 items-center justify-center rounded-2xl">
                <Icon name="target" size={26} />
              </span>
              <h2 className="font-display text-[26px] font-bold text-ink">
                Culprit found
              </h2>
              <p className="font-mono text-sm text-ink">
                {fileName(finished.candidate.relativePath)}
              </p>
              <p className="text-sm leading-relaxed text-ink-secondary">
                It has been moved to Quarantine with the reason
                "Troubleshooter: confirmed culprit." Every other file is back
                at its exact original path, hash-verified. Launch the game and
                enjoy the quiet.
              </p>
              <div className="flex justify-center gap-3">
                <Button onClick={() => props.onNavigate("quarantine")}>
                  Open Quarantine
                </Button>
                <Button variant="quiet" onClick={() => setFinished(null)}>
                  Start another hunt
                </Button>
              </div>
            </div>
          ) : outcome === "inconclusive" ? (
            <div className="space-y-4 text-center">
              <span className="icon-chip mx-auto flex h-14 w-14 items-center justify-center rounded-2xl">
                <Icon name="alert" size={24} />
              </span>
              <h2 className="font-display text-[26px] font-bold text-ink">
                Inconclusive — and that's the honest answer
              </h2>
              <p className="text-sm leading-relaxed text-ink-secondary">
                The problem happened even with the last suspect removed, which
                usually means two or more files interact, or the cause lives
                outside your packages and scripts. Everything has been
                restored exactly as it was. Running another session after
                removing what you suspect most is a good next move.
              </p>
              <div className="flex justify-center">
                <Button variant="quiet" onClick={() => setFinished(null)}>
                  Back to the assistant
                </Button>
              </div>
            </div>
          ) : outcome === "no_problem" ? (
            <div className="space-y-4 text-center">
              <span className="icon-chip mx-auto flex h-14 w-14 items-center justify-center rounded-2xl">
                <Icon name="quarantine" size={24} />
              </span>
              <h2 className="font-display text-[26px] font-bold text-ink">
                Nothing to hunt
              </h2>
              <p className="text-sm leading-relaxed text-ink-secondary">
                The problem didn't happen with everything in place, so the
                session closed without moving a single file. If it comes back,
                the assistant is right here.
              </p>
              <div className="flex justify-center">
                <Button variant="quiet" onClick={() => setFinished(null)}>
                  Back to the assistant
                </Button>
              </div>
            </div>
          ) : (
            <div className="space-y-4 text-center">
              <span className="icon-chip mx-auto flex h-14 w-14 items-center justify-center rounded-2xl">
                <Icon name="backups" size={24} />
              </span>
              <h2 className="font-display text-[26px] font-bold text-ink">
                Hunt aborted — everything restored
              </h2>
              <p className="text-sm leading-relaxed text-ink-secondary">
                Every file is back at its exact original path, hash-verified.
                No harm done; come back whenever you're ready.
              </p>
              <div className="flex justify-center">
                <Button variant="quiet" onClick={() => setFinished(null)}>
                  Back to the assistant
                </Button>
              </div>
            </div>
          )}
        </Card>
      </div>
    );
  }

  // ---------------------------------------------------------------- intro
  if (!session) {
    return (
      <div className="mx-auto max-w-2xl space-y-6">
        <Card finials>
          <SectionTitle>The 50/50 assistant</SectionTitle>
          <p className="text-sm leading-relaxed text-ink-secondary">
            Something's wrong in the game and one of your files is
            responsible. The assistant runs the classic 50/50 hunt for you:
            it sets aside half your packages and scripts, you test the game
            and say whether the problem is still there, and it keeps halving
            until one suspect remains. You never touch a file yourself.
          </p>
          <div className="mt-4 space-y-2">
            {[
              "Every move is hash-verified and journaled — a changed file refuses to move.",
              "Close the app mid-hunt, come back tomorrow: the session resumes exactly where it rested.",
              "Abort at any moment and every file returns to its exact original path.",
            ].map((line) => (
              <div key={line} className="flex items-start gap-2 text-sm text-ink-secondary">
                <span
                  aria-hidden="true"
                  className="mt-0.5 text-[11px] text-gold [text-shadow:0_0_8px_rgba(201,164,92,0.9)]"
                >
                  ✦
                </span>
                {line}
              </div>
            ))}
          </div>
          <div className="mt-5">
            <Field
              label="What's going wrong? (optional)"
              hint="A note for future-you — it's shown while the session is running."
            >
              <input
                value={note}
                onChange={(e) => setNote(e.target.value)}
                placeholder="e.g. Game crashes when Sims travel to Sulani"
                className="w-full max-w-md rounded-control border border-border-subtle bg-surface px-3 py-2 text-sm text-ink placeholder:text-ink-muted"
              />
            </Field>
          </div>
          <div className="mt-5 flex items-center gap-3">
            <Button
              disabled={busy}
              onClick={() =>
                void run(() => troubleshootStart(note.trim() || undefined))
              }
            >
              {busy ? "Preparing…" : "Begin the hunt"}
            </Button>
            <span className="text-xs text-ink-muted">
              Starting moves nothing — the first question is whether the
              problem even happens.
            </span>
          </div>
        </Card>
      </div>
    );
  }

  // --------------------------------------------------------------- active
  const testsLeft = estimatedTests(session);
  const phasePanel = (() => {
    switch (session.phase) {
      case "baseline":
        return {
          title: "First — confirm the problem",
          body: `All ${session.total.toLocaleString()} files are in place; nothing has moved. Launch The Sims 4 and check whether the problem happens.`,
          present: {
            title: "The problem happens",
            sub: "Good — the hunt can begin. The first half will be set aside.",
          },
          gone: {
            title: "It's gone",
            sub: "Nothing to hunt; the session closes without touching a file.",
          },
        };
      case "testing":
        return {
          title: `Round ${session.round} — test the game`,
          body: `${session.outCount.toLocaleString()} files are set aside, ${session.inCount.toLocaleString()} are in place. Launch the game and try to reproduce the problem.`,
          present: {
            title: "Problem still happens",
            sub: "The culprit is among the files currently in place.",
          },
          gone: {
            title: "The problem is gone",
            sub: "The culprit is among the set-aside files.",
          },
        };
      default:
        return {
          title: "The decisive test",
          body: "Only the file below is set aside — every other file, including everything exonerated along the way, is back in place. Test one more time.",
          present: {
            title: "Problem still happens",
            sub: "Not just this file — the session ends honestly as inconclusive and everything is restored.",
          },
          gone: {
            title: "The problem is gone",
            sub: "Culprit confirmed — it moves to Quarantine and everything else stays home.",
          },
        };
    }
  })();

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      {reconcile && reconcile.healed > 0 ? (
        <Banner tone="info">
          Picked up where you left off — {reconcile.healed}{" "}
          {reconcile.healed === 1 ? "record" : "records"} healed from disk.
        </Banner>
      ) : null}
      {reconcile &&
      (reconcile.conflicts.length > 0 || reconcile.missing.length > 0) ? (
        <Banner tone="danger">
          The session and the disk disagree:{" "}
          {reconcile.conflicts.length > 0
            ? `${reconcile.conflicts.length} file(s) exist in both places`
            : ""}
          {reconcile.conflicts.length > 0 && reconcile.missing.length > 0
            ? "; "
            : ""}
          {reconcile.missing.length > 0
            ? `${reconcile.missing.length} file(s) found in neither`
            : ""}
          . Run a scan, then review before continuing:{" "}
          {[...reconcile.conflicts, ...reconcile.missing]
            .slice(0, 3)
            .map(fileName)
            .join(", ")}
          {reconcile.conflicts.length + reconcile.missing.length > 3
            ? "…"
            : ""}
        </Banner>
      ) : null}

      <Card finials>
        <div className="flex items-baseline justify-between gap-3">
          <div className="min-w-0 flex-1">
            <SectionTitle>
              {session.phase === "confirming"
                ? "Confirmation"
                : session.phase === "baseline"
                  ? "Baseline"
                  : `Hunting — round ${session.round}`}
            </SectionTitle>
          </div>
          <span className="shrink-0 text-xs text-ink-muted">
            ≈ {testsLeft} {testsLeft === 1 ? "test" : "tests"} to go
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
          <div>
            <div className="font-display text-[30px] font-bold leading-tight text-ink [text-shadow:0_1px_0_#fff]">
              {session.poolSize.toLocaleString()}
            </div>
            <div className="text-[10.5px] font-bold uppercase tracking-[0.13em] text-[#94875e]">
              Suspects remaining
            </div>
          </div>
          <div>
            <div className="font-display text-[30px] font-bold leading-tight text-ink [text-shadow:0_1px_0_#fff]">
              {session.outCount.toLocaleString()}
            </div>
            <div className="text-[10.5px] font-bold uppercase tracking-[0.13em] text-[#94875e]">
              Set aside
            </div>
          </div>
          <div className="ml-auto">
            <StepDots done={session.round} remaining={testsLeft} />
          </div>
        </div>
        {session.problemNote ? (
          <p className="mt-3 text-xs italic text-ink-muted">
            Hunting: "{session.problemNote}"
          </p>
        ) : null}
      </Card>

      <Card finials>
        <SectionTitle>{phasePanel.title}</SectionTitle>
        <p className="text-sm leading-relaxed text-ink-secondary">
          {phasePanel.body}
        </p>
        {session.phase === "confirming" && session.candidate ? (
          <p className="mt-3 rounded-control border border-gold/40 bg-gold/10 px-3 py-2 text-center font-mono text-sm text-ink">
            {fileName(session.candidate.relativePath)}
          </p>
        ) : null}
        {isGameRunning ? (
          <div className="mt-4">
            <Pill tone="warning">
              The Sims 4 is running — close it before reporting a verdict, so
              files can move safely.
            </Pill>
          </div>
        ) : null}
        {busy && progress && progress.total > 0 ? (
          <div className="mt-4">
            <div className="flex items-baseline justify-between text-xs text-ink-muted">
              <span>Arranging files — every move hash-verified…</span>
              <span>
                {progress.done.toLocaleString()} /{" "}
                {progress.total.toLocaleString()}
              </span>
            </div>
            <div className="raised-pill mt-1.5 h-2.5 overflow-hidden rounded-full border border-gold/50 bg-surface">
              <div
                className="h-full rounded-full transition-[width] duration-150"
                style={{
                  width: `${Math.round((progress.done / progress.total) * 100)}%`,
                  backgroundImage: "var(--gold-grad-soft)",
                  boxShadow: "0 0 8px rgba(201,164,92,0.6)",
                }}
              />
            </div>
          </div>
        ) : null}
        <div className="mt-5 flex flex-col gap-3 sm:flex-row">
          <VerdictTile
            icon="conflicts"
            title={busy ? "Arranging…" : phasePanel.present.title}
            sub={phasePanel.present.sub}
            disabled={verdictLocked}
            onClick={() =>
              void run(() => troubleshootVerdict(session.id, true))
            }
          />
          <VerdictTile
            icon="quarantine"
            title={busy ? "Arranging…" : phasePanel.gone.title}
            sub={phasePanel.gone.sub}
            disabled={verdictLocked}
            onClick={() =>
              void run(() => troubleshootVerdict(session.id, false))
            }
          />
        </div>
      </Card>

      <Card frame={false} className="border border-border-subtle">
        {confirmingAbort ? (
          <div className="flex flex-wrap items-center justify-between gap-3">
            <span className="text-sm text-ink-secondary">
              Abort the hunt? Every file returns to its exact original path,
              hash-verified.
            </span>
            <span className="flex gap-2">
              <Button
                variant="quiet"
                disabled={busy || isGameRunning}
                onClick={() =>
                  void run(() => troubleshootAbort(session.id))
                }
              >
                {busy ? "Restoring…" : "Yes — restore everything"}
              </Button>
              <Button variant="quiet" onClick={() => setConfirmingAbort(false)}>
                Keep hunting
              </Button>
            </span>
          </div>
        ) : (
          <div className="flex items-center justify-between gap-3">
            <span className="text-xs text-ink-muted">
              Session #{session.id} · started{" "}
              {new Date(session.createdAt).toLocaleDateString()}
            </span>
            <Button variant="quiet" disabled={busy} onClick={() => setConfirmingAbort(true)}>
              Abort & restore everything
            </Button>
          </div>
        )}
      </Card>
    </div>
  );
}
