import { Search } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { useSearchParams } from "react-router";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { fill, usePrefs } from "@/lib/prefs";
import { web, type SearchResult } from "@/lib/web";
import { MessageLine } from "./MessageLine";
import { ErrorPanel, Skeleton } from "./States";

export function SearchTab({ org, slug }: { org: string; slug: string }) {
  const { t } = usePrefs();
  const s = t.app.search;
  const [params, setParams] = useSearchParams();
  const q = params.get("q") ?? "";
  const [draft, setDraft] = useState(q);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // The query lives in the URL (?q=), so a search can be shared or reloaded.
  useEffect(() => {
    setDraft(q);
    if (!q.trim()) {
      setResult(null);
      return;
    }
    const c = new AbortController();
    setLoading(true);
    setError(null);
    web
      .search(org, slug, q.trim(), 20, c.signal)
      .then((r) => setResult(r))
      .catch((e) => {
        if (!c.signal.aborted) setError(e);
      })
      .finally(() => {
        if (!c.signal.aborted) setLoading(false);
      });
    return () => c.abort();
  }, [org, slug, q]);

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const next = new URLSearchParams(params);
    if (draft.trim()) next.set("q", draft.trim());
    else next.delete("q");
    setParams(next);
    if (!draft.trim()) inputRef.current?.focus();
  };

  return (
    <div className="flex flex-col gap-6">
      <form role="search" onSubmit={onSubmit} className="flex flex-col gap-2">
        <label htmlFor="search-q" className="text-sm font-medium">
          {s.label}
        </label>
        <div className="flex gap-2">
          <div className="relative min-w-0 flex-1">
            <Search className="pointer-events-none absolute top-1/2 left-3.5 size-4 -translate-y-1/2 text-fg-muted" aria-hidden="true" />
            <Input
              id="search-q"
              ref={inputRef}
              type="search"
              name="q"
              autoComplete="off"
              spellCheck={false}
              enterKeyHint="search"
              placeholder={s.placeholder}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              className="pl-10"
            />
          </div>
          <Button type="submit" size="lg" disabled={loading}>
            {loading ? s.searching : s.submit}
          </Button>
        </div>
        <p className="text-sm text-fg-muted">{s.hint}</p>
      </form>

      <div aria-live="polite">
        {loading && !result ? (
          <div className="space-y-2">
            <Skeleton className="h-10" />
            <Skeleton className="h-10" />
            <Skeleton className="h-10 w-2/3" />
          </div>
        ) : error ? (
          <ErrorPanel error={error} />
        ) : result ? (
          result.hits.length === 0 ? (
            <p className="text-fg-muted">{fill(s.none, { q: result.query })}</p>
          ) : (
            <div className="flex flex-col gap-3">
              <p className="text-sm text-fg-muted">{fill(s.count, { n: result.hits.length, q: result.query })}</p>
              <ol className="divide-y divide-rule overflow-hidden rounded-lg border border-rule bg-bg-raised">
                {result.hits.map((h) => (
                  <li key={h.id} className="flex items-start gap-3 pr-4">
                    <div className="min-w-0 flex-1 py-1">
                      <MessageLine id={h.id} createdAt={h.created_at} author={h.author} trust={h.trust} body={h.snippet} clamp />
                    </div>
                    <span className="tnum mt-2.5 shrink-0 font-mono text-[0.68rem] text-fg-muted" title={s.score}>
                      {h.score.toFixed(2)}
                    </span>
                  </li>
                ))}
              </ol>
            </div>
          )
        ) : null}
      </div>
    </div>
  );
}
