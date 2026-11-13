import { Check, Copy } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";

type Kind = "meta" | "hunk" | "add" | "del" | "ctx";

function classify(lines: string[]): Kind[] {
  let inHunk = false;
  let header = false;
  return lines.map((l, i) => {
    if (header) {
      header = false;
      return "meta";
    }
    if (l.startsWith("@@")) {
      inHunk = true;
      return "hunk";
    }
    // A "--- a" / "+++ b" pair starts a file, before its first hunk.
    if (l.startsWith("--- ") && lines[i + 1]?.startsWith("+++ ")) {
      inHunk = false;
      header = true;
      return "meta";
    }
    if (!inHunk || l.startsWith("\\")) return "meta";
    if (l.startsWith("+")) return "add";
    if (l.startsWith("-")) return "del";
    return "ctx";
  });
}

const ROW: Record<Kind, string> = {
  meta: "text-fg-muted",
  hunk: "bg-accent/10 text-accent",
  add: "bg-ok/15 text-fg",
  del: "bg-danger/12 text-fg",
  ctx: "text-fg",
};

const SIGN: Record<Kind, string> = {
  meta: "text-fg-muted",
  hunk: "text-accent",
  add: "text-ok font-semibold",
  del: "text-danger font-semibold",
  ctx: "text-fg-muted",
};

/** A unified diff with +/− line colouring. Long lines scroll sideways. */
export function DiffView({ diff, label }: { diff: string; label: string }) {
  const lines = diff.replace(/\n$/, "").split("\n");
  const kinds = classify(lines);
  return (
    <div
      tabIndex={0}
      role="region"
      aria-label={label}
      className="overflow-x-auto rounded-md border border-rule bg-bg-raised py-1.5 [scrollbar-width:thin]"
    >
      <pre translate="no" className="text-[0.72rem] leading-[1.75]">
        <code className="inline-block min-w-full">
          {lines.map((l, i) => {
            const k = kinds[i] ?? "ctx";
            const signed = k === "add" || k === "del" || k === "ctx";
            const sign = signed ? (l[0] === " " ? " " : (l[0] ?? " ")) : "";
            const rest = signed ? l.slice(1) : l;
            return (
              <span key={i} className={cn("block pr-4 whitespace-pre", ROW[k], signed ? "pl-1" : "pl-3")}>
                {signed ? <span className={cn("inline-block w-5 text-center select-none", SIGN[k])}>{k === "del" ? "−" : sign}</span> : null}
                {rest || " "}
              </span>
            );
          })}
        </code>
      </pre>
    </div>
  );
}

/** A command the plan will run, shown as a shell line that wraps, with a copy button beside it. */
export function CommandLine({ command }: { command: string }) {
  const { t } = usePrefs();
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setCopied(false);
    }
  };
  return (
    <div className="flex items-start gap-2 rounded-md border border-rule bg-bg-raised py-1.5 pr-1.5 pl-3">
      <pre translate="no" className="min-w-0 flex-1 py-1 text-[0.72rem] leading-[1.75] break-all whitespace-pre-wrap">
        <span aria-hidden="true" className="text-accent select-none">
          ${" "}
        </span>
        {command}
      </pre>
      <button
        type="button"
        onClick={() => void copy()}
        aria-label={t.code.copyLabel}
        className="inline-flex h-7 shrink-0 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs text-fg-muted transition-colors hover:bg-bg-sunk hover:text-fg"
      >
        {copied ? <Check className="size-3.5" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
        <span aria-live="polite">{copied ? t.code.copied : t.code.copy}</span>
      </button>
    </div>
  );
}
