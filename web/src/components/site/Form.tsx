import { CircleAlert, Eye, EyeOff, LoaderCircle } from "lucide-react";
import { forwardRef, useId, useState, type ComponentProps, type ReactNode } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";

type FieldProps = Omit<ComponentProps<"input">, "id"> & {
  label: string;
  error?: string | null;
  hint?: ReactNode;
  /** Extra text at the right of the label row (e.g. a live counter). */
  aside?: ReactNode;
};

export const Field = forwardRef<HTMLInputElement, FieldProps>(function Field(
  { label, error, hint, aside, type, className, ...props },
  ref,
) {
  const { t } = usePrefs();
  const id = useId();
  const hintId = `${id}-hint`;
  const errId = `${id}-err`;
  const [shown, setShown] = useState(false);
  const isPassword = type === "password";
  const describedBy = [error ? errId : null, hint ? hintId : null].filter(Boolean).join(" ") || undefined;

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <div className="flex items-baseline justify-between gap-3">
        <Label htmlFor={id}>{label}</Label>
        {aside}
      </div>
      <div className="relative">
        <Input
          ref={ref}
          id={id}
          type={isPassword && shown ? "text" : type}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy}
          className={isPassword ? "pr-12" : undefined}
          spellCheck={false}
          {...props}
        />
        {isPassword ? (
          <button
            type="button"
            onClick={() => setShown((s) => !s)}
            aria-label={shown ? t.auth.hide : t.auth.show}
            aria-pressed={shown}
            className="absolute top-1/2 right-1.5 inline-flex size-9 -translate-y-1/2 cursor-pointer items-center justify-center rounded-md text-fg-muted transition-colors hover:bg-bg-sunk hover:text-fg touch-manipulation"
          >
            {shown ? <EyeOff className="size-4" aria-hidden="true" /> : <Eye className="size-4" aria-hidden="true" />}
          </button>
        ) : null}
      </div>
      {error ? (
        <p id={errId} className="flex items-start gap-1.5 text-sm text-danger">
          <CircleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          {error}
        </p>
      ) : null}
      {hint ? (
        <p id={hintId} className="text-sm text-fg-muted">
          {hint}
        </p>
      ) : null}
    </div>
  );
});

export const FormAlert = forwardRef<HTMLDivElement, { title: string; message: string | null }>(function FormAlert(
  { title, message },
  ref,
) {
  return (
    <div ref={ref} tabIndex={-1} role="alert" aria-live="assertive" className="outline-none empty:hidden">
      {message ? (
        <div className="mb-6 flex gap-3 rounded-md border border-danger/40 bg-danger-soft px-4 py-3 text-sm">
          <CircleAlert className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden="true" />
          <div>
            <p className="font-medium text-fg">{title}</p>
            <p className="mt-0.5 text-fg">{message}</p>
          </div>
        </div>
      ) : null}
    </div>
  );
});

export function SubmitLabel({ pending, idle, busy }: { pending: boolean; idle: string; busy: string }) {
  return pending ? (
    <>
      <LoaderCircle className="size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
      {busy}
    </>
  ) : (
    <>{idle}</>
  );
}

export const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
