import { CircleCheck, FileDiff, FilePlus2, Info, LoaderCircle, RotateCw, SquareTerminal, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { errorText, type Applied, type Change, type Plan } from "../bridge";
import { useDesktop } from "../state";
import { InlineError, Mono } from "./bits";
import { CommandLine, DiffView } from "./Diff";

export type ReviewRequest = {
  /** Sheet title, like "Connect Claude Code". */
  title: string;
  /** Shown when the plan has no changes, like "Already connected". */
  empty: string;
  plan: () => Promise<Plan>;
  apply: () => Promise<Applied>;
  onApplied?: (a: Applied) => void;
};

type Phase =
  | { k: "planning" }
  | { k: "planError"; message: string }
  | { k: "ready"; plan: Plan; applying: boolean; error: string | null }
  | { k: "done"; backups: string[] };

/** Side sheet that shows a plan, then applies it. A native modal dialog: focus is trapped, Esc cancels. */
export function ReviewSheet({ request, onClose }: { request: ReviewRequest; onClose: () => void }) {
  const { t } = usePrefs();
  const r = t.desktop.review;
  const { setStatus } = useDesktop();
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const [phase, setPhase] = useState<Phase>({ k: "planning" });
  const busy = phase.k === "ready" && phase.applying;

  useEffect(() => {
    const d = ref.current;
    if (d && !d.open) d.showModal();
  }, []);

  const loadPlan = useCallback(
    (signal: { off: boolean }) => {
      setPhase({ k: "planning" });
      request.plan().then(
        (plan) => !signal.off && setPhase({ k: "ready", plan, applying: false, error: null }),
        (err) => !signal.off && setPhase({ k: "planError", message: errorText(err) }),
      );
    },
    [request],
  );

  useEffect(() => {
    const signal = { off: false };
    loadPlan(signal);
    return () => {
      signal.off = true;
    };
  }, [loadPlan]);

  const close = () => ref.current?.close();

  const apply = async () => {
    if (phase.k !== "ready" || phase.applying) return;
    setPhase({ ...phase, applying: true, error: null });
    try {
      const a = await request.apply();
      setStatus(a.status);
      request.onApplied?.(a);
      setPhase({ k: "done", backups: a.backups });
    } catch (err) {
      setPhase({ ...phase, applying: false, error: errorText(err) });
    }
  };

  const n = phase.k === "ready" ? phase.plan.changes.length : 0;

  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      aria-busy={phase.k === "planning" || busy}
      onCancel={(e) => {
        if (busy) e.preventDefault();
      }}
      onClose={onClose}
      className="fixed inset-y-0 right-0 left-auto m-0 flex h-dvh max-h-none w-[min(46rem,100vw)] max-w-none flex-col border-l border-rule-strong bg-bg p-0 text-fg shadow-[0_0_48px_-12px_hsl(var(--shadow-color)/0.45)] transition-[translate,opacity] duration-300 ease-out-expo outline-none backdrop:bg-[oklch(0.18_0.06_268/0.42)] starting:translate-x-8 starting:opacity-0 [&:not([open])]:hidden"
    >
      <header className="flex items-start gap-3 border-b border-rule px-5 py-4">
        <div className="min-w-0 flex-1">
          <h2 id={titleId} className="text-lg font-semibold">
            {request.title}
          </h2>
          <p className="mt-0.5 text-sm text-fg-muted">
            {phase.k === "ready" && n > 0 ? `${fill(n === 1 ? r.countOne : r.countOther, { n })} · ${r.lede}` : phase.k === "planning" ? r.planning : null}
          </p>
        </div>
        <Button variant="ghost" size="icon" className="-mt-1 -mr-2 size-9" onClick={close} disabled={busy} aria-label={r.close}>
          <X aria-hidden="true" />
        </Button>
      </header>

      <div className="flex-1 overflow-y-auto px-5 py-5 [scrollbar-width:thin]">
        {phase.k === "planning" ? (
          <p role="status" className="inline-flex items-center gap-2.5 text-sm text-fg-muted">
            <LoaderCircle className="size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            {r.planning}
          </p>
        ) : phase.k === "planError" ? (
          <InlineError title={r.planFailed} message={phase.message}>
            <Button variant="outline" size="sm" className="mt-3" onClick={() => loadPlan({ off: false })}>
              <RotateCw aria-hidden="true" />
              {r.retry}
            </Button>
          </InlineError>
        ) : phase.k === "done" ? (
          <div role="status" className="flex flex-col gap-4">
            <p className="flex items-center gap-2.5 font-medium">
              <CircleCheck className="size-5 text-ok" aria-hidden="true" />
              {r.doneTitle}
            </p>
            {phase.backups.length ? (
              <div>
                <p className="text-sm text-fg-muted">{r.backups}</p>
                <ul className="mt-2 flex flex-col gap-1 rounded-md border border-rule bg-bg-raised px-3 py-2">
                  {phase.backups.map((b) => (
                    <li key={b}>
                      <Mono>{b}</Mono>
                    </li>
                  ))}
                </ul>
              </div>
            ) : (
              <p className="text-sm text-fg-muted">{r.noBackups}</p>
            )}
          </div>
        ) : phase.plan.changes.length === 0 ? (
          <p role="status" className="flex items-center gap-2.5">
            <CircleCheck className="size-5 text-ok" aria-hidden="true" />
            {request.empty}
          </p>
        ) : (
          <div className="flex flex-col gap-6">
            <ol className="flex flex-col gap-5">
              {phase.plan.changes.map((c, i) => (
                <ChangeItem key={`${i}-${c.path ?? c.title}`} change={c} />
              ))}
            </ol>
            {phase.plan.notes.length ? (
              <section aria-label={r.notes} className="rounded-md border border-rule bg-bg-sunk/60 px-4 py-3">
                <h3 className="text-sm font-medium">{r.notes}</h3>
                <ul className="mt-1.5 flex flex-col gap-1.5 text-sm text-fg-muted">
                  {phase.plan.notes.map((note) => (
                    <li key={note} className="flex gap-2">
                      <Info className="mt-[0.2rem] size-3.5 shrink-0 text-accent" aria-hidden="true" />
                      <span>{note}</span>
                    </li>
                  ))}
                </ul>
              </section>
            ) : null}
            {phase.error ? <InlineError title={r.applyFailed} message={phase.error} /> : null}
          </div>
        )}
      </div>

      <footer className="flex flex-wrap items-center justify-end gap-2 border-t border-rule bg-bg-sunk/40 px-5 py-3">
        {phase.k === "ready" && phase.plan.changes.length > 0 ? (
          <>
            <Button variant="ghost" size="sm" onClick={close} disabled={busy}>
              {r.cancel}
            </Button>
            <Button size="sm" className="min-w-24" onClick={() => void apply()} disabled={busy}>
              <SubmitLabel pending={busy} idle={r.apply} busy={r.applying} />
            </Button>
          </>
        ) : phase.k === "done" ? (
          <Button size="sm" className="min-w-24" onClick={close} autoFocus>
            {r.done}
          </Button>
        ) : (
          <Button variant="outline" size="sm" onClick={close} autoFocus={phase.k === "ready"}>
            {r.close}
          </Button>
        )}
      </footer>
    </dialog>
  );
}

function ChangeItem({ change }: { change: Change }) {
  const { t } = usePrefs();
  const r = t.desktop.review;
  const Icon = change.kind === "command" ? SquareTerminal : change.new_file ? FilePlus2 : FileDiff;
  return (
    <li className="flex flex-col gap-2">
      <div className="flex items-start gap-2.5">
        <Icon className="mt-0.5 size-4 shrink-0 text-accent" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[0.92rem] font-medium">
            {change.title}
            {change.new_file ? (
              <span className="rounded-[4px] border border-ok/45 px-1.5 py-px font-mono text-[0.65rem] font-normal text-ok">{r.newFile}</span>
            ) : null}
          </p>
          {change.path ? <Mono className="text-fg-muted">{change.path}</Mono> : change.kind === "command" ? <span className="text-[0.78rem] text-fg-muted">{r.command}</span> : null}
        </div>
      </div>
      {change.kind === "command" ? (
        <CommandLine command={change.diff} />
      ) : (
        <DiffView diff={change.diff} label={change.path ?? change.title} />
      )}
    </li>
  );
}
