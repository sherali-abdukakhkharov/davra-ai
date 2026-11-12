import { ChevronRight, FileText, Search } from "lucide-react";
import { useDeferredValue, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useFormat } from "@/lib/format";
import { fill, usePrefs } from "@/lib/prefs";
import type { Handoff } from "@/lib/web";
import { Markdown } from "./Markdown";

const PAGE = 30;

export function HandoffsTab({ handoffs }: { handoffs: Handoff[] }) {
  const { t } = usePrefs();
  const h = t.app.handoffs;
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(PAGE);
  const q = useDeferredValue(query.trim().toLowerCase());

  const shown = useMemo(() => {
    if (!q) return handoffs;
    return handoffs.filter((x) =>
      [x.ticket ?? "", x.from, x.to.join(" "), x.body, x.source_path ?? ""].join("\n").toLowerCase().includes(q),
    );
  }, [handoffs, q]);

  if (handoffs.length === 0) {
    return <p className="rounded-lg border border-dashed border-rule-strong px-6 py-10 text-center text-fg-muted">{h.empty}</p>;
  }
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <label htmlFor="handoff-filter" className="sr-only">
          {h.filter}
        </label>
        <div className="relative w-full max-w-[24rem]">
          <Search className="pointer-events-none absolute top-1/2 left-3.5 size-4 -translate-y-1/2 text-fg-muted" aria-hidden="true" />
          <Input
            id="handoff-filter"
            type="search"
            autoComplete="off"
            spellCheck={false}
            placeholder={h.filterPlaceholder}
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setLimit(PAGE);
            }}
            className="h-10 pl-10"
          />
        </div>
        <p className="tnum text-sm text-fg-muted" aria-live="polite">
          {fill(h.count, { n: shown.length, total: handoffs.length })}
        </p>
      </div>
      {shown.length === 0 ? (
        <p className="text-fg-muted">{fill(t.app.search.none, { q: query.trim() })}</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {shown.slice(0, limit).map((x) => (
            <HandoffItem key={x.id} x={x} />
          ))}
        </ul>
      )}
      {shown.length > limit ? (
        <div>
          <Button variant="outline" onClick={() => setLimit((n) => n + PAGE)}>
            {fill(h.more, { n: Math.min(PAGE, shown.length - limit) })}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

/** First meaningful line of the body, used as the collapsed preview. */
function preview(body: string) {
  const line = body
    .split("\n")
    .map((l) => l.replace(/^#+\s*|^[-*+]\s+|^>\s?/g, "").replace(/[*_`]/g, "").trim())
    .find(Boolean);
  return line ?? "";
}

function HandoffItem({ x }: { x: Handoff }) {
  const { t } = usePrefs();
  const h = t.app.handoffs;
  const f = useFormat();
  const [open, setOpen] = useState(false);
  return (
    <li>
      <details
        onToggle={(e) => setOpen((e.currentTarget as HTMLDetailsElement).open)}
        className="group overflow-hidden rounded-lg border border-rule bg-bg-raised open:shadow-[0_12px_32px_-20px_hsl(var(--shadow-color)/0.4)]"
      >
        <summary className="grid cursor-pointer list-none grid-cols-[auto_1fr] gap-x-3 gap-y-1 px-4 py-3.5 select-none sm:px-5 [&::-webkit-details-marker]:hidden hover:bg-bg-sunk/60">
          <ChevronRight className="mt-0.5 size-4 shrink-0 text-fg-muted transition-transform duration-200 group-open:rotate-90" aria-hidden="true" />
          <span className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5">
            <span translate="no" className="font-mono text-[0.78rem] font-medium">
              {x.ticket ?? h.noTicket}
            </span>
            <span translate="no" className="inline-flex flex-wrap items-center gap-1.5 font-mono text-[0.75rem]">
              <span className="rounded-[4px] bg-bg-sunk px-1.5 py-px">{x.from}</span>
              <span className="text-fg-muted" aria-hidden="true">
                →
              </span>
              <span className="sr-only">{h.to}</span>
              {x.to.map((r) => (
                <span key={r} className="rounded-[4px] bg-bg-sunk px-1.5 py-px">
                  {r}
                </span>
              ))}
            </span>
            <time dateTime={x.created_at} title={f.full(x.created_at)} className="text-[0.8rem] text-fg-muted sm:ml-auto">
              {fill(h.opened, { time: f.relative(x.created_at) })}
            </time>
          </span>
          {!open ? <span className="col-start-2 truncate text-sm text-fg-muted">{preview(x.body)}</span> : null}
        </summary>
        {open ? (
          <div className="border-t border-rule px-4 py-4 sm:px-5 sm:pl-12">
            <Markdown source={x.body} />
            {x.source_path ? (
              <p className="mt-5 flex items-center gap-2 font-mono text-[0.7rem] text-fg-muted" translate="no">
                <FileText className="size-3.5 shrink-0" aria-hidden="true" />
                <span className="sr-only">{h.file}: </span>
                <span className="min-w-0 truncate" title={x.source_path}>
                  {x.source_path}
                </span>
              </p>
            ) : null}
          </div>
        ) : null}
      </details>
    </li>
  );
}
