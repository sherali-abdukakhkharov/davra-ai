import { Check, Copy } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";

type Props = {
  lines: string[];
  label?: string;
  className?: string;
  tone?: "field" | "ground";
};

/** Shell snippet with a copy button. Lines starting with # render as comments. */
export function CodeBlock({ lines, label, className, tone = "ground" }: Props) {
  const { t } = usePrefs();
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copy = async () => {
    const text = lines.filter((l) => !l.startsWith("#")).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setCopied(false);
    }
  };

  const field = tone === "field";
  const button = (
    <button
      type="button"
      onClick={copy}
      aria-label={t.code.copyLabel}
      className={cn(
        "inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs transition-colors touch-manipulation",
        field ? "text-field-muted hover:bg-field-deep hover:text-field-fg" : "text-fg-muted hover:bg-bg-sunk hover:text-fg",
      )}
    >
      {copied ? <Check className="size-3.5" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
      <span className="sr-only sm:not-sr-only" aria-live="polite">
        {copied ? t.code.copied : t.code.copy}
      </span>
    </button>
  );
  return (
    <figure
      className={cn(
        "group relative overflow-hidden rounded-lg border",
        field ? "border-field-rule bg-field-panel text-field-fg" : "border-rule bg-bg-raised text-fg",
        className,
      )}
    >
      {label ? (
        <figcaption
          className={cn(
            "flex min-h-10 items-center justify-between gap-3 border-b py-1 pr-1.5 pl-4 text-xs",
            field ? "border-field-rule text-field-muted" : "border-rule text-fg-muted",
          )}
        >
          <span>{label}</span>
          {button}
        </figcaption>
      ) : null}
      <pre translate="no" className={cn("overflow-x-auto [scrollbar-width:thin] px-4 py-3.5 text-[0.78rem] leading-[1.9]", !label && "pr-14")}>
        <code>
          {lines.map((l, i) => (
            <span key={i} className="block whitespace-pre-wrap [overflow-wrap:anywhere] sm:whitespace-pre">
              {l.startsWith("#") ? (
                <span className={field ? "text-field-muted" : "text-fg-muted"}>{l}</span>
              ) : (
                <>
                  <span aria-hidden="true" className={cn("select-none", field ? "text-glaze" : "text-accent")}>
                    ${" "}
                  </span>
                  {l}
                </>
              )}
            </span>
          ))}
        </code>
      </pre>
      {label ? null : <div className="absolute top-2.5 right-2">{button}</div>}
    </figure>
  );
}
