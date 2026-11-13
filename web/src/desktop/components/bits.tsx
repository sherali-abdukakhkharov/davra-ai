import { CircleAlert } from "lucide-react";
import type { ReactNode } from "react";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { inTauri } from "../bridge";

export type Tone = "ok" | "warn" | "danger" | "off";

/** Small dot used in the sidebar and on status pills. */
export function Dot({ tone, className }: { tone: Tone; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-block size-2 shrink-0 rounded-full",
        tone === "ok" && "bg-ok",
        tone === "warn" && "bg-saffron",
        tone === "danger" && "bg-danger",
        tone === "off" && "border border-fg-muted/70 bg-transparent",
        className,
      )}
    />
  );
}

export function StatusPill({ tone, children, className }: { tone: Tone; children: ReactNode; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2 py-px text-[0.75rem] leading-5 whitespace-nowrap",
        tone === "ok" && "border-ok/45 text-fg",
        tone === "warn" && "border-saffron bg-saffron/15 text-fg",
        tone === "danger" && "border-danger/45 bg-danger-soft text-fg",
        tone === "off" && "border-dashed border-rule-strong text-fg-muted",
        className,
      )}
    >
      <Dot tone={tone} />
      {children}
    </span>
  );
}

export function PanelHeader({ id, title, sub, action }: { id: string; title: string; sub?: ReactNode; action?: ReactNode }) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
      <div className="min-w-0 max-w-[40rem]">
        <h1 id={id} tabIndex={-1} className="text-xl font-semibold tracking-[-0.01em] outline-none">
          {title}
        </h1>
        {sub ? <p className="mt-1 text-sm text-fg-muted">{sub}</p> : null}
      </div>
      {action}
    </header>
  );
}

export function Switch({
  checked,
  onChange,
  disabled,
  labelledBy,
  describedBy,
  id,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  labelledBy?: string;
  describedBy?: string;
  id?: string;
}) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-6 w-10 shrink-0 cursor-pointer items-center rounded-full border transition-colors duration-150 disabled:cursor-not-allowed disabled:opacity-60",
        checked ? "border-primary bg-primary" : "border-rule-strong bg-bg-sunk",
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "inline-block size-4 rounded-full shadow-sm transition-transform duration-150",
          checked ? "translate-x-[1.1rem] bg-primary-fg" : "translate-x-[0.2rem] bg-fg-muted",
        )}
      />
    </button>
  );
}

export const selectClass =
  "h-10 w-full min-w-0 cursor-pointer rounded-md border border-rule-strong bg-bg-raised px-3 text-[0.92rem] text-fg outline-none transition-[border-color,box-shadow] duration-150 hover:border-fg-muted/60 focus-visible:border-accent focus-visible:ring-3 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60";

export function InlineError({ title, message, className, children }: { title: string; message: string; className?: string; children?: ReactNode }) {
  return (
    <div role="alert" className={cn("flex gap-3 rounded-md border border-danger/40 bg-danger-soft px-4 py-3 text-sm", className)}>
      <CircleAlert className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <p className="font-medium text-fg">{title}</p>
        <p className="mt-0.5 break-words text-fg">{message}</p>
        {children}
      </div>
    </div>
  );
}

/** Shown only when the UI runs in a browser against the mock bridge. */
export function MockBadge({ className }: { className?: string }) {
  const { t } = usePrefs();
  if (inTauri) return null;
  return (
    <span
      title={t.desktop.mockHint}
      className={cn("rounded-full border border-dashed border-saffron px-2 py-0.5 font-mono text-[0.65rem] whitespace-nowrap text-saffron-ink", className)}
    >
      {t.desktop.mockBadge}
    </span>
  );
}

/** A file path or command in mono that may wrap anywhere. */
export function Mono({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span translate="no" className={cn("font-mono text-[0.78rem] [overflow-wrap:anywhere]", className)}>
      {children}
    </span>
  );
}

/** One label/value row in a definition list card. */
export function DefRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid items-center gap-1 px-4 py-3 sm:grid-cols-[9rem_minmax(0,1fr)] sm:gap-4">
      <dt className="text-sm text-fg-muted">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  );
}
