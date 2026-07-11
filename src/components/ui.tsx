/**
 * Small shared primitives in the design language: soft surfaces, rounded
 * corners, calm colors, status never conveyed by color alone.
 */
import type { ReactNode } from "react";
import mark from "../assets/mark.png";

export function Card(props: { children: ReactNode; className?: string }) {
  return (
    <section
      className={`elev-card rounded-card border border-border-subtle p-5 ${props.className ?? ""}`}
    >
      {props.children}
    </section>
  );
}

export function SectionTitle(props: { children: ReactNode; hint?: string }) {
  return (
    <div className="mb-3">
      <div className="flex items-center gap-3">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-sage-deep">
          {props.children}
        </h2>
        <span
          aria-hidden="true"
          className="h-px flex-1 bg-gradient-to-r from-gold/50 to-transparent"
        />
      </div>
      {props.hint ? (
        <p className="mt-0.5 text-xs text-ink-muted">{props.hint}</p>
      ) : null}
    </div>
  );
}

type ButtonVariant = "primary" | "soft" | "quiet" | "danger";

const buttonStyles: Record<ButtonVariant, string> = {
  primary:
    "btn-jewel text-white disabled:bg-border-strong disabled:text-ink-muted disabled:shadow-none",
  soft: "bg-sage-soft text-sage-deep hover:bg-sage hover:text-white disabled:opacity-50",
  quiet:
    "bg-transparent text-ink-secondary hover:bg-soft disabled:opacity-50",
  danger:
    "bg-blush-soft text-danger hover:bg-danger hover:text-white disabled:opacity-50",
};

export function Button(props: {
  children: ReactNode;
  onClick?: () => void;
  variant?: ButtonVariant;
  disabled?: boolean;
  title?: string;
  type?: "button" | "submit";
}) {
  return (
    <button
      type={props.type ?? "button"}
      onClick={props.onClick}
      disabled={props.disabled}
      title={props.title}
      className={`rounded-control px-4 py-2 text-sm font-medium transition-colors ${buttonStyles[props.variant ?? "primary"]}`}
    >
      {props.children}
    </button>
  );
}

type Tone = "sage" | "blue" | "rose" | "neutral" | "warning" | "danger";

const pillTones: Record<Tone, string> = {
  sage: "bg-sage-soft text-sage-deep",
  blue: "bg-blue-soft text-muted-blue-deep",
  rose: "bg-blush-soft text-dusty-rose",
  neutral: "bg-soft text-ink-secondary",
  warning: "bg-blue-soft text-warning",
  danger: "bg-blush-soft text-danger",
};

export function Pill(props: { children: ReactNode; tone?: Tone; title?: string }) {
  return (
    <span
      title={props.title}
      className={`inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs font-medium ${pillTones[props.tone ?? "neutral"]}`}
    >
      {props.children}
    </span>
  );
}

export function Toggle(props: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  hint?: string;
  disabled?: boolean;
}) {
  return (
    <label className="flex cursor-pointer items-start justify-between gap-4 py-2">
      <span>
        <span className="block text-sm font-medium text-ink">{props.label}</span>
        {props.hint ? (
          <span className="block text-xs text-ink-muted">{props.hint}</span>
        ) : null}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={props.checked}
        aria-label={props.label}
        disabled={props.disabled}
        onClick={() => props.onChange(!props.checked)}
        className={`relative mt-0.5 h-6 w-11 shrink-0 rounded-full transition-colors ${
          props.checked ? "bg-sage" : "bg-border-strong"
        } disabled:opacity-50`}
      >
        <span
          className={`absolute top-0.5 h-5 w-5 rounded-full bg-white shadow-card transition-all ${
            props.checked ? "left-[22px]" : "left-0.5"
          }`}
        />
      </button>
    </label>
  );
}

export function Field(props: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="py-2">
      <div className="mb-1 text-sm font-medium text-ink">{props.label}</div>
      {props.children}
      {props.hint ? (
        <p className="mt-1 text-xs text-ink-muted">{props.hint}</p>
      ) : null}
    </div>
  );
}

export function TextInput(props: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  ariaLabel?: string;
}) {
  return (
    <input
      type="text"
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
      placeholder={props.placeholder}
      aria-label={props.ariaLabel ?? props.placeholder}
      className="w-full rounded-control border border-border-subtle bg-surface px-3 py-2 text-sm text-ink placeholder:text-ink-muted"
    />
  );
}

export function Banner(props: {
  tone: "info" | "warning" | "danger" | "success";
  children: ReactNode;
  onDismiss?: () => void;
}) {
  const tones = {
    info: "bg-blue-soft text-muted-blue-deep",
    warning: "bg-blue-soft text-warning",
    danger: "bg-blush-soft text-danger",
    success: "bg-sage-soft text-sage-deep",
  } as const;
  return (
    <div
      role="status"
      className={`flex items-start justify-between gap-3 rounded-card px-4 py-3 text-sm ${tones[props.tone]}`}
    >
      <div className="leading-relaxed">{props.children}</div>
      {props.onDismiss ? (
        <button
          type="button"
          onClick={props.onDismiss}
          aria-label="Dismiss"
          className="rounded-control px-2 text-xs font-semibold opacity-70 hover:opacity-100"
        >
          ✕
        </button>
      ) : null}
    </div>
  );
}

export function EmptyState(props: { title: string; body: string; children?: ReactNode }) {
  return (
    <div className="rounded-card border border-dashed border-border-strong bg-soft p-8 text-center">
      <p className="text-sm font-semibold text-ink">{props.title}</p>
      <p className="mx-auto mt-1 max-w-md text-sm text-ink-secondary">{props.body}</p>
      {props.children ? <div className="mt-4">{props.children}</div> : null}
    </div>
  );
}

export function Stat(props: {
  label: string;
  value: string;
  sub?: string;
  icon?: IconName;
}) {
  return (
    <div className="gold-edge-card rounded-card px-4 py-3">
      <div className="flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-ink-muted">
        {props.icon ? (
          <span className="icon-chip flex h-7 w-7 items-center justify-center rounded-lg">
            <Icon name={props.icon} size={15} className="text-gold-deep" />
          </span>
        ) : null}
        {props.label}
      </div>
      <div className="mt-0.5 text-xl font-semibold text-ink">{props.value}</div>
      {props.sub ? <div className="text-xs text-ink-muted">{props.sub}</div> : null}
    </div>
  );
}

/** The brand mark used in the sidebar, onboarding, and notices. */
export function PlumbobMark(props: { size?: number }) {
  const s = props.size ?? 28;
  return (
    <img
      src={mark}
      width={s}
      height={s}
      alt=""
      aria-hidden="true"
      className="shrink-0 drop-shadow-[0_0_18px_rgba(72,214,140,0.45)] drop-shadow-[0_6px_12px_rgba(0,0,0,0.35)]"
    />
  );
}

export function Modal(props: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-[rgba(41,50,56,0.35)] p-6"
      onClick={props.onClose}
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        onClick={(e) => e.stopPropagation()}
        className={`max-h-[85vh] w-full ${props.wide ? "max-w-2xl" : "max-w-lg"} overflow-y-auto rounded-card border border-border-subtle bg-surface p-5 shadow-raised`}
      >
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-base font-semibold text-ink">{props.title}</h2>
          <button
            type="button"
            onClick={props.onClose}
            aria-label="Close"
            className="rounded-control px-2 py-1 text-sm text-ink-muted hover:bg-soft hover:text-ink"
          >
            ✕
          </button>
        </div>
        {props.children}
      </div>
    </div>
  );
}

export type IconName =
  | "dashboard" | "library" | "duplicates" | "conflicts" | "quarantine"
  | "backups" | "activity" | "settings" | "calendar" | "profiles"
  | "file" | "database" | "package" | "code" | "archive" | "alert"
  | "lock" | "help" | "sparkle";

const ICON_PATHS: Record<IconName, ReactNode> = {
  dashboard: (
    <>
      <path d="M4 14a8 8 0 0 1 16 0" />
      <path d="M12 14l3.5-3.5" />
      <path d="M4.5 18h15" />
    </>
  ),
  library: (
    <>
      <path d="M4 8l8-4 8 4-8 4-8-4z" />
      <path d="M4 12l8 4 8-4" />
      <path d="M4 16l8 4 8-4" />
    </>
  ),
  duplicates: (
    <>
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15V5a2 2 0 0 1 2-2h10" />
    </>
  ),
  conflicts: <path d="M13 2L4 14h6l-1 8 9-12h-6l1-8z" />,
  quarantine: <path d="M12 3l7 3v5c0 5-3.5 8-7 10-3.5-2-7-5-7-10V6l7-3z" />,
  backups: (
    <>
      <rect x="3" y="4" width="18" height="4" rx="1" />
      <path d="M5 8v11a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8" />
      <path d="M10 12h4" />
    </>
  ),
  activity: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7.5V12l3 2" />
    </>
  ),
  settings: (
    <>
      <path d="M4 6h16M4 12h16M4 18h16" />
      <circle cx="14" cy="6" r="1.9" fill="var(--background-sidebar)" />
      <circle cx="8" cy="12" r="1.9" fill="var(--background-sidebar)" />
      <circle cx="16" cy="18" r="1.9" fill="var(--background-sidebar)" />
    </>
  ),
  calendar: (
    <>
      <rect x="4" y="5" width="16" height="16" rx="2" />
      <path d="M4 10h16M8 3v4M16 3v4" />
    </>
  ),
  profiles: (
    <>
      <circle cx="9" cy="8" r="3.5" />
      <path d="M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6" />
      <path d="M16.5 6.9a2.6 2.6 0 1 1 0 4.6M21 20c0-2.5-1.6-4.7-3.8-5.5" />
    </>
  ),
  file: (
    <>
      <path d="M6 3h8l4 4v13a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z" />
      <path d="M14 3v5h5" />
    </>
  ),
  database: (
    <>
      <ellipse cx="12" cy="5.5" rx="7.5" ry="3" />
      <path d="M4.5 5.5v13c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3v-13" />
      <path d="M4.5 12c0 1.7 3.4 3 7.5 3s7.5-1.3 7.5-3" />
    </>
  ),
  package: (
    <>
      <path d="M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3z" />
      <path d="M12 12l8-4.5M12 12v9M12 12L4 7.5" />
    </>
  ),
  code: <path d="M8 8l-4 4 4 4M16 8l4 4-4 4" />,
  archive: (
    <>
      <path d="M4 7l8-4 8 4v10l-8 4-8-4V7z" />
      <path d="M4 7l8 4 8-4M12 11v10" />
    </>
  ),
  alert: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 8v5M12 16.5h.01" />
    </>
  ),
  lock: (
    <>
      <rect x="6" y="11" width="12" height="9" rx="2" />
      <path d="M8.5 11V8a3.5 3.5 0 0 1 7 0v3" />
    </>
  ),
  help: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M9.6 9.4a2.5 2.5 0 1 1 3.3 2.4c-.8.3-1.4.9-1.4 1.7v.4M11.5 16.8h.01" />
    </>
  ),
  sparkle: (
    <path
      d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8L12 3z"
      fill="currentColor"
      stroke="none"
    />
  ),
};

/** Hand-drawn line icon set in the Motherlode style — thin strokes, gold
 * where accents apply. Decorative by default. */
export function Icon(props: {
  name: IconName;
  size?: number;
  className?: string;
}) {
  const s = props.size ?? 18;
  return (
    <svg
      width={s}
      height={s}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={`shrink-0 ${props.className ?? ""}`}
    >
      {ICON_PATHS[props.name]}
    </svg>
  );
}

function Flourish(props: { className?: string }) {
  return (
    <svg
      width="46"
      height="46"
      viewBox="0 0 48 48"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      aria-hidden="true"
      className={props.className}
    >
      <path d="M6 42 C6 16 16 6 42 6" />
      <path d="M14 34 C14 20 20 14 34 14" strokeWidth={0.9} opacity={0.7} />
      <circle cx="9" cy="39" r="1.3" fill="currentColor" stroke="none" />
      <circle cx="39" cy="9" r="1.3" fill="currentColor" stroke="none" />
    </svg>
  );
}

/** The gilded frame around the content area — double hairline with corner
 * flourishes, per the Motherlode design spec. Purely decorative. */
export function OrnamentalFrame() {
  return (
    <div aria-hidden="true" className="pointer-events-none absolute inset-0 z-20 text-gold/70">
      <div
        className="absolute inset-2 rounded-[14px] shadow-[0_0_22px_rgba(201,164,92,0.14)]"
        style={{
          border: "1px solid transparent",
          backgroundImage: "linear-gradient(#0000,#0000), var(--gold-grad-soft)",
          backgroundClip: "padding-box, border-box",
          backgroundOrigin: "padding-box, border-box",
        }}
      />
      <div className="absolute inset-[13px] rounded-[10px] border border-gold/20" />
      <Flourish className="absolute left-1 top-1" />
      <Flourish className="absolute right-1 top-1 rotate-90" />
      <Flourish className="absolute bottom-1 right-1 rotate-180" />
      <Flourish className="absolute bottom-1 left-1 -rotate-90" />
    </div>
  );
}
