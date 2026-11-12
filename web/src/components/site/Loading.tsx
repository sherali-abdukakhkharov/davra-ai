import { usePrefs } from "@/lib/prefs";

export function PageLoading({ label }: { label?: string }) {
  const { t } = usePrefs();
  return (
    <div className="flex min-h-dvh items-center justify-center" role="status" aria-live="polite">
      <span className="inline-flex items-center gap-3 text-fg-muted">
        <span
          className="size-4 animate-spin rounded-full border-2 border-rule-strong border-t-accent motion-reduce:animate-none"
          aria-hidden="true"
        />
        {label ?? t.app.loading}
      </span>
    </div>
  );
}
