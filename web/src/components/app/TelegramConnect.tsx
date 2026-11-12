import { ArrowUpRight, Check, Send } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { web, webErrorText, type TelegramLink } from "@/lib/web";

type Phase =
  | { kind: "idle" }
  | { kind: "starting" }
  | { kind: "waiting"; link: TelegramLink; until: number }
  | { kind: "expired" }
  | { kind: "done" }
  | { kind: "error"; message: string };

const POLL_MS = 3000;

function mmss(ms: number) {
  const s = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/**
 * Links the signed-in account to the person's Telegram id: the hub hands out a deep link,
 * the person presses Start in the bot, and we poll the overview until it says linked.
 */
export function TelegramConnect({ poll }: { poll: () => Promise<boolean> }) {
  const { t } = usePrefs();
  const c = t.app.telegram;
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [now, setNow] = useState(() => Date.now());
  const pollRef = useRef(poll);
  pollRef.current = poll;

  useEffect(() => {
    if (phase.kind !== "waiting") return;
    const tick = window.setInterval(() => setNow(Date.now()), 1000);
    let stopped = false;
    const run = async () => {
      if (stopped) return;
      if (Date.now() > phase.until) {
        setPhase({ kind: "expired" });
        return;
      }
      const linked = await pollRef.current().catch(() => false);
      if (stopped) return;
      if (linked) setPhase({ kind: "done" });
      else timer = window.setTimeout(run, POLL_MS);
    };
    let timer = window.setTimeout(run, POLL_MS);
    return () => {
      stopped = true;
      window.clearInterval(tick);
      window.clearTimeout(timer);
    };
  }, [phase]);

  const start = async () => {
    setPhase({ kind: "starting" });
    try {
      const link = await web.telegramLink();
      setNow(Date.now());
      setPhase({ kind: "waiting", link, until: Date.now() + link.expires_in * 1000 });
    } catch (err) {
      setPhase({ kind: "error", message: webErrorText(err, t.app.errors) });
    }
  };

  if (phase.kind === "done") {
    return (
      <div role="status" className="flex items-center gap-3 rounded-lg border border-ok/40 bg-bg-raised px-5 py-4">
        <Check className="size-5 text-ok" aria-hidden="true" />
        <p>{c.done}</p>
      </div>
    );
  }

  return (
    <section
      data-field
      aria-labelledby="tg-title"
      className="relative overflow-hidden rounded-lg bg-field px-5 py-6 text-field-fg sm:px-8 sm:py-8"
    >
      <div className="grid gap-6 *:min-w-0 md:grid-cols-[1fr_auto] md:items-center md:gap-10">
        <div>
          <h2 id="tg-title" className="display flex items-center gap-3 text-[1.6rem] sm:text-[1.9rem]">
            <Send className="size-6 shrink-0 text-glaze" aria-hidden="true" />
            {c.title}
          </h2>
          <p className="mt-3 max-w-[40rem] text-field-muted">{c.body}</p>
          {phase.kind === "waiting" ? (
            <div className="mt-5 space-y-2" aria-live="polite">
              <p className="text-field-fg">{c.step}</p>
              <p className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-field-muted">
                <span className="inline-flex items-center gap-2">
                  <span className="size-2 animate-pulse rounded-full bg-saffron motion-reduce:animate-none" aria-hidden="true" />
                  <span className="tnum">{fill(c.waiting, { time: mmss(phase.until - now) })}</span>
                </span>
                <span>
                  {c.code}: <code translate="no" className="text-field-fg">{phase.link.code}</code>
                </span>
              </p>
            </div>
          ) : null}
          {phase.kind === "expired" ? (
            <p role="alert" className="mt-5 text-saffron">
              {c.expired}
            </p>
          ) : null}
          {phase.kind === "error" ? (
            <p role="alert" className="mt-5 text-saffron">
              {phase.message}
            </p>
          ) : null}
        </div>
        <div className="flex flex-wrap gap-3 md:flex-col md:items-stretch">
          {phase.kind === "waiting" ? (
            <>
              <Button asChild variant="saffron" size="lg">
                <a href={phase.link.deep_link} target="_blank" rel="noreferrer">
                  {fill(c.open, { bot: phase.link.bot })}
                  <ArrowUpRight aria-hidden="true" />
                </a>
              </Button>
              <Button variant="fieldGhost" onClick={() => setPhase({ kind: "idle" })}>
                {c.cancel}
              </Button>
            </>
          ) : (
            <Button variant="saffron" size="lg" onClick={start} disabled={phase.kind === "starting"}>
              {phase.kind === "starting" ? c.starting : phase.kind === "idle" ? c.start : c.again}
            </Button>
          )}
        </div>
      </div>
    </section>
  );
}
