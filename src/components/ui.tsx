/**
 * Small shared primitives in the design language: soft surfaces, rounded
 * corners, calm colors, status never conveyed by color alone.
 */
import type { ReactNode } from "react";
import mark from "../assets/mark.png";

export function Card(props: { children: ReactNode; className?: string }) {
  return (
    <section
      className={`rounded-card border border-border-subtle bg-surface p-5 shadow-card ${props.className ?? ""}`}
    >
      {props.children}
    </section>
  );
}

export function SectionTitle(props: { children: ReactNode; hint?: string }) {
  return (
    <div className="mb-3">
      <h2 className="text-sm font-semibold uppercase tracking-wider text-sage-deep">
        {props.children}
      </h2>
      {props.hint ? (
        <p className="mt-0.5 text-xs text-ink-muted">{props.hint}</p>
      ) : null}
    </div>
  );
}

type ButtonVariant = "primary" | "soft" | "quiet" | "danger";

const buttonStyles: Record<ButtonVariant, string> = {
  primary:
    "bg-sage text-white hover:bg-sage-deep disabled:bg-border-strong disabled:text-ink-muted",
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

export function Stat(props: { label: string; value: string; sub?: string }) {
  return (
    <div className="rounded-card bg-soft px-4 py-3">
      <div className="text-xs font-medium uppercase tracking-wide text-ink-muted">
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
      className="shrink-0 rounded-lg"
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
