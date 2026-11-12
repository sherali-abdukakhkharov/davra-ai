import { CircleAlert, RotateCw } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { webErrorText } from "@/lib/web";

export function ErrorPanel({ error, onRetry, className }: { error: unknown; onRetry?: () => void; className?: string }) {
  const { t } = usePrefs();
  return (
    <div role="alert" className={cn("flex flex-col items-start gap-4 rounded-lg border border-danger/40 bg-danger-soft px-5 py-4", className)}>
      <div className="flex gap-3">
        <CircleAlert className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden="true" />
        <div>
          <p className="font-medium">{t.app.errors.title}</p>
          <p className="mt-0.5 text-sm">{webErrorText(error, t.app.errors)}</p>
        </div>
      </div>
      {onRetry ? (
        <Button variant="outline" size="sm" onClick={onRetry}>
          <RotateCw aria-hidden="true" />
          {t.app.errors.retry}
        </Button>
      ) : null}
    </div>
  );
}

/** Quiet placeholder blocks while data loads. */
export function Skeleton({ className }: { className?: string }) {
  return <div aria-hidden="true" className={cn("animate-pulse rounded-md bg-bg-sunk motion-reduce:animate-none", className)} />;
}

export function Notice({ tone = "info", children }: { tone?: "info" | "ok" | "warn"; children: ReactNode }) {
  return (
    <p
      role="status"
      className={cn(
        "rounded-md border px-3.5 py-2.5 text-sm",
        tone === "ok" && "border-ok/40 text-fg",
        tone === "warn" && "border-saffron bg-saffron/15 text-fg",
        tone === "info" && "border-rule-strong text-fg-muted",
      )}
    >
      {children}
    </p>
  );
}

export function RoleBadge({ role, className }: { role: string; className?: string }) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-[4px] border px-1.5 py-px font-mono text-[0.68rem] leading-[1.35]",
        role === "owner" ? "border-saffron bg-saffron/20 text-fg" : "border-rule-strong text-fg-muted",
        className,
      )}
    >
      {role}
    </span>
  );
}
