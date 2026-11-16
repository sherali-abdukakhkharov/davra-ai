import { ArrowUpRight, Check, Copy, Send, TriangleAlert, Unlink } from "lucide-react";
import { Fragment, useEffect, useId, useRef, useState, type FormEvent, type ReactNode, type RefObject } from "react";
import { SubmitLabel } from "@/components/site/Form";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { fill, usePrefs } from "@/lib/prefs";
import { web, webErrorText, type ProjectRef, type TelegramGroupLink, type TelegramStatus, type TgTopic } from "@/lib/web";
import { Confirm, selectClass } from "./MembersTab";
import { ErrorPanel, Skeleton } from "./States";
import { mmss, POLL_MS } from "./TelegramConnect";
import { useResource } from "./useResource";

type Props = {
  project: ProjectRef;
  /**
   * "members": always shown, with a note when the group can't be linked.
   * "room": only shown while there is something to do (not linked, and you may link it).
   */
  placement?: "members" | "room";
  /** The group changed; the project header should refresh. */
  onChanged: () => void;
};

/** Fills `{key}` slots in a translated string with elements, keeping word order per language. */
function rich(template: string, slots: Record<string, ReactNode>): ReactNode {
  return template.split(/\{(\w+)\}/g).map((part, i) => <Fragment key={i}>{i % 2 ? slots[part] ?? `{${part}}` : part}</Fragment>);
}

/** Focus fell to <body> (its element went away) or is still inside `root`. */
function focusIsLost(root: HTMLElement | null) {
  const a = document.activeElement;
  return !a || a === document.body || Boolean(root?.contains(a));
}

/**
 * The project's Telegram group: owners and maintainers link it with a one-time deep link
 * (Telegram asks which group to add the bot to), then choose the forum topic the bot posts to.
 */
export function TelegramGroup({ project, placement = "members", onChanged }: Props) {
  const { t } = usePrefs();
  const c = t.app.tgGroup;
  const { org, slug } = project;
  const res = useResource((s) => web.telegramStatus(org, slug, s), [org, slug]);
  const s = res.data;
  const [said, setSaid] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);
  // In the room, the panel appears only for an unlinked group you may link, then stays for this visit.
  const [engaged, setEngaged] = useState(false);
  if (s && placement === "room" && !engaged && s.bot && !s.linked && s.can_link) setEngaged(true);

  const update = (next: TelegramStatus, message: string) => {
    res.setData(() => next);
    setSaid(message);
    onChanged();
  };

  // Linking or unlinking swaps the whole panel; land on its heading instead of <body>.
  const linked = s?.linked;
  const prevLinked = useRef(linked);
  useEffect(() => {
    const prev = prevLinked.current;
    prevLinked.current = linked;
    if (prev === undefined || linked === undefined || prev === linked) return;
    if (focusIsLost(rootRef.current)) headingRef.current?.focus();
  }, [linked]);

  if (!s) {
    if (res.error) return placement === "members" ? <ErrorPanel error={res.error} onRetry={() => void res.reload()} /> : null;
    return <Skeleton className="h-36" />;
  }
  if (placement === "room" && !engaged) return null;

  let body: ReactNode;
  if (!s.bot) body = <Quiet>{c.noBot}</Quiet>;
  else if (s.linked) body = <Linked project={project} status={s} bot={s.bot} headingRef={headingRef} onStatus={update} />;
  else if (s.can_link) body = <LinkFlow project={project} headingRef={headingRef} onLinked={(next) => update(next, c.linked)} />;
  else body = <Quiet>{c.cantLink}</Quiet>;

  return (
    <div ref={rootRef} className="min-w-0">
      {body}
      <p role="status" aria-live="polite" className="sr-only">
        {said}
      </p>
    </div>
  );
}

function Quiet({ children }: { children: ReactNode }) {
  return (
    <p className="flex items-start gap-2 text-sm text-fg-muted">
      <Send className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
      {children}
    </p>
  );
}

function CopyButton({ text, label, targetId, variant }: { text: string; label: string; targetId: string; variant: "outline" | "fieldOutline" }) {
  const { t } = usePrefs();
  const c = t.app.tgGroup;
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      const el = document.getElementById(targetId);
      if (el) window.getSelection()?.selectAllChildren(el);
    }
  };
  return (
    <Button variant={variant} onClick={() => void copy()} className="h-auto min-h-10 shrink-0" aria-label={label}>
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
      <span className="hidden sm:inline" aria-live="polite">
        {copied ? c.copied : c.copy}
      </span>
    </Button>
  );
}

type Phase =
  | { kind: "idle" }
  | { kind: "starting" }
  | { kind: "waiting"; link: TelegramGroupLink; until: number }
  | { kind: "expired" }
  | { kind: "error"; message: string };

/** Not linked yet: hand out a deep link, show the manual command, and poll until the bot reports the group. */
function LinkFlow({
  project,
  headingRef,
  onLinked,
}: {
  project: ProjectRef;
  headingRef: RefObject<HTMLHeadingElement | null>;
  onLinked: (s: TelegramStatus) => void;
}) {
  const { t } = usePrefs();
  const c = t.app.tgGroup;
  const { org, slug } = project;
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [now, setNow] = useState(() => Date.now());
  const onLinkedRef = useRef(onLinked);
  onLinkedRef.current = onLinked;
  const panelRef = useRef<HTMLElement>(null);
  const startRef = useRef<HTMLButtonElement>(null);
  const openRef = useRef<HTMLAnchorElement>(null);
  const titleId = useId();
  const commandId = useId();

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
      const next = await web.telegramStatus(org, slug).catch(() => null);
      if (stopped) return;
      if (next?.linked) onLinkedRef.current(next);
      else timer = window.setTimeout(run, POLL_MS);
    };
    let timer = window.setTimeout(run, POLL_MS);
    return () => {
      stopped = true;
      window.clearInterval(tick);
      window.clearTimeout(timer);
    };
  }, [phase, org, slug]);

  // The buttons swap with each phase; keep focus on the one that is there now (not on first mount).
  const prevKind = useRef(phase.kind);
  useEffect(() => {
    if (prevKind.current === phase.kind) return;
    prevKind.current = phase.kind;
    if (!focusIsLost(panelRef.current)) return;
    if (phase.kind === "waiting") openRef.current?.focus();
    else if (phase.kind !== "starting") startRef.current?.focus();
  }, [phase.kind]);

  const start = async () => {
    setPhase({ kind: "starting" });
    try {
      const link = await web.telegramGroupLink(org, slug);
      setNow(Date.now());
      setPhase({ kind: "waiting", link, until: Date.now() + link.expires_in * 1000 });
    } catch (err) {
      setPhase({ kind: "error", message: webErrorText(err, t.app.errors) });
    }
  };

  return (
    <section
      ref={panelRef}
      data-field
      aria-labelledby={titleId}
      className="relative overflow-hidden rounded-lg bg-field px-5 py-6 text-field-fg sm:px-8 sm:py-7"
    >
      <div className="grid gap-6 *:min-w-0 md:grid-cols-[1fr_auto] md:items-start md:gap-10">
        <div>
          <h2 id={titleId} ref={headingRef} tabIndex={-1} className="display flex items-center gap-3 text-[1.5rem] outline-none sm:text-[1.75rem]">
            <Send className="size-5 shrink-0 text-glaze" aria-hidden="true" />
            {c.title}
          </h2>
          <p className="mt-3 max-w-[40rem] text-field-muted">{c.body}</p>
          {phase.kind === "waiting" ? (
            <div className="mt-5 space-y-4">
              <div className="space-y-2" aria-live="polite">
                <p className="text-field-fg">{fill(c.step, { bot: phase.link.bot })}</p>
                <p className="inline-flex items-center gap-2 text-sm text-field-muted">
                  <span className="size-2 animate-pulse rounded-full bg-saffron motion-reduce:animate-none" aria-hidden="true" />
                  <span className="tnum">{fill(c.waiting, { time: mmss(phase.until - now) })}</span>
                </p>
              </div>
              <div className="space-y-2">
                <p className="text-sm text-field-muted">{c.manual}</p>
                <div className="flex items-stretch gap-2">
                  <code
                    id={commandId}
                    translate="no"
                    className="min-w-0 flex-1 rounded-md border border-field-rule bg-field-deep px-3.5 py-2.5 font-mono text-[0.8rem] break-all text-field-fg select-all"
                  >
                    {phase.link.command}
                  </code>
                  <CopyButton text={phase.link.command} label={c.copyCommand} targetId={commandId} variant="fieldOutline" />
                </div>
                <p className="text-sm text-field-muted">{c.onlyYou}</p>
              </div>
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
                <a ref={openRef} href={phase.link.deep_link} target="_blank" rel="noreferrer">
                  {c.open}
                  <ArrowUpRight aria-hidden="true" />
                </a>
              </Button>
              <Button variant="fieldGhost" onClick={() => setPhase({ kind: "idle" })}>
                {c.cancel}
              </Button>
            </>
          ) : (
            <Button ref={startRef} variant="saffron" size="lg" onClick={() => void start()} disabled={phase.kind === "starting"}>
              {phase.kind === "starting" ? c.starting : phase.kind === "idle" ? c.start : c.again}
            </Button>
          )}
        </div>
      </div>
    </section>
  );
}

/** Linked: where the bot posts, the topic picker, and unlinking. */
function Linked({
  project,
  status: s,
  bot,
  headingRef,
  onStatus,
}: {
  project: ProjectRef;
  status: TelegramStatus;
  bot: string;
  headingRef: RefObject<HTMLHeadingElement | null>;
  onStatus: (s: TelegramStatus, message: string) => void;
}) {
  const { t } = usePrefs();
  const c = t.app.tgGroup;
  const titleId = useId();
  const name = (x: TgTopic) => x.name ?? fill(c.topicUnnamed, { id: x.id });
  const where = s.forum ? (s.topic ? name(s.topic) : c.general) : c.wholeGroup;

  return (
    <section aria-labelledby={titleId} className="rounded-lg border border-rule-strong bg-bg-raised">
      <div className="px-5 pt-5 sm:px-6 sm:pt-6">
        <h2 id={titleId} ref={headingRef} tabIndex={-1} className="flex items-center gap-2 text-lg font-semibold outline-none">
          <Send className="size-4 text-accent" aria-hidden="true" />
          {c.title}
        </h2>
        <p className="mt-2 text-[1.05rem] font-medium break-words">{s.chat_title ?? c.untitled}</p>
        <p className="mt-1 text-sm text-fg-muted">{rich(c.goesTo, { where: <strong className="font-medium text-fg">{where}</strong> })}</p>
      </div>

      <div className="flex flex-col gap-5 px-5 py-5 sm:px-6">
        {s.reads_all === false ? (
          <p className="flex items-start gap-2.5 rounded-md border border-saffron bg-saffron/15 px-3.5 py-2.5 text-sm text-fg">
            <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
            <span>{fill(c.notAdmin, { bot })}</span>
          </p>
        ) : null}
        {!s.can_manage ? (
          <p className="text-sm text-fg-muted">{c.cantManage}</p>
        ) : s.forum ? (
          <Topics project={project} status={s} bot={bot} name={name} onStatus={onStatus} />
        ) : (
          <p className="text-sm text-fg-muted">{c.noForum}</p>
        )}
      </div>

      {s.can_manage ? <UnlinkRow project={project} status={s} chat={s.chat_title ?? c.untitled} onStatus={onStatus} /> : null}
    </section>
  );
}

function Topics({
  project,
  status: s,
  bot,
  name,
  onStatus,
}: {
  project: ProjectRef;
  status: TelegramStatus;
  bot: string;
  name: (x: TgTopic) => string;
  onStatus: (s: TelegramStatus, message: string) => void;
}) {
  const { t } = usePrefs();
  const c = t.app.tgGroup;
  const { org, slug } = project;
  const selectId = useId();
  const inputId = useId();
  const [choice, setChoice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // The current topic may be one the bot never listed (set with /here before a rename, say).
  const options = s.topic && !s.topics.some((x) => x.id === s.topic!.id) ? [s.topic, ...s.topics] : s.topics;
  const current = s.topic ? String(s.topic.id) : "";

  const pick = async (value: string) => {
    const n = ++seq.current;
    setChoice(value);
    setError(null);
    try {
      const next = await web.telegramTopic(org, slug, { id: value === "" ? null : Number(value) });
      if (n !== seq.current) return;
      onStatus(next, fill(c.saved, { where: next.topic ? name(next.topic) : c.general }));
    } catch (err) {
      if (n === seq.current) setError(webErrorText(err, t.app.errors));
    } finally {
      if (n === seq.current) setChoice(null);
    }
  };

  const create = async (e: FormEvent) => {
    e.preventDefault();
    const topic = newName.trim();
    if (!topic || creating) return;
    setCreating(true);
    setCreateError(null);
    try {
      const next = await web.telegramTopic(org, slug, { create: topic });
      setNewName("");
      onStatus(next, fill(c.created, { name: topic }));
    } catch (err) {
      setCreateError(webErrorText(err, t.app.errors));
    } finally {
      setCreating(false);
      inputRef.current?.focus();
    }
  };

  return (
    <div className="flex flex-col gap-5">
      <div className="grid gap-5 *:min-w-0 md:grid-cols-2">
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3">
            <Label htmlFor={selectId}>{c.topicLabel}</Label>
            {choice !== null ? (
              <span className="inline-flex items-center gap-1.5 text-sm text-fg-muted" aria-hidden="true">
                <SubmitLabel pending idle="" busy={c.saving} />
              </span>
            ) : null}
          </div>
          <select
            id={selectId}
            value={choice ?? current}
            onChange={(e) => void pick(e.target.value)}
            aria-busy={choice !== null}
            aria-describedby={error ? `${selectId}-error` : undefined}
            className={selectClass}
          >
            <option value="">{c.general}</option>
            {options.map((x) => (
              <option key={x.id} value={String(x.id)}>
                {name(x)}
              </option>
            ))}
          </select>
          {error ? (
            <p id={`${selectId}-error`} role="alert" className="text-sm text-danger">
              {error}
            </p>
          ) : null}
        </div>

        <form noValidate onSubmit={create} aria-busy={creating} className="flex flex-col gap-2">
          <Label htmlFor={inputId}>{c.newTopic}</Label>
          <div className="flex flex-col gap-2 sm:flex-row">
            <Input
              ref={inputRef}
              id={inputId}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder={c.newTopicPlaceholder}
              maxLength={128}
              autoComplete="off"
              aria-invalid={createError ? true : undefined}
              aria-describedby={`${inputId}-hint${createError ? ` ${inputId}-error` : ""}`}
              className="h-11 text-[0.95rem]"
            />
            <Button type="submit" variant="outline" disabled={creating || !newName.trim()}>
              <SubmitLabel pending={creating} idle={c.create} busy={c.creating} />
            </Button>
          </div>
          <p id={`${inputId}-hint`} className="text-sm text-fg-muted">
            {c.createHint}
          </p>
          {createError ? (
            <p id={`${inputId}-error`} role="alert" className="text-sm text-danger">
              {createError}
            </p>
          ) : null}
        </form>
      </div>
      <p className="text-sm text-fg-muted">
        {rich(c.here, {
          cmd: (
            <code translate="no" className="rounded-[4px] bg-bg-sunk px-1.5 py-0.5 font-mono text-[0.8rem] text-fg select-all">
              /here@{bot}
            </code>
          ),
        })}
      </p>
    </div>
  );
}

function UnlinkRow({
  project,
  status,
  chat,
  onStatus,
}: {
  project: ProjectRef;
  status: TelegramStatus;
  chat: string;
  onStatus: (s: TelegramStatus, message: string) => void;
}) {
  const { t } = usePrefs();
  const c = t.app.tgGroup;
  const [confirm, setConfirm] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const backToButton = useRef(false);

  useEffect(() => {
    if (!confirm && backToButton.current) buttonRef.current?.focus();
    backToButton.current = false;
  }, [confirm]);

  const unlink = async () => {
    setPending(true);
    setError(null);
    try {
      await web.telegramUnlink(project.org, project.slug);
      // Ask for the fresh state; if that fails, the group is still gone.
      const next = await web.telegramStatus(project.org, project.slug).catch(() => null);
      onStatus(next ?? { ...status, linked: false, chat_title: null, forum: false, topic: null, topics: [], can_manage: false, reads_all: null }, c.unlinked);
    } catch (err) {
      setPending(false);
      setError(webErrorText(err, t.app.errors));
    }
  };

  return (
    <div className="border-t border-rule px-5 py-4 sm:px-6">
      {confirm ? (
        <Confirm
          text={fill(c.unlinkConfirm, { chat })}
          yes={c.unlinkYes}
          busy={c.unlinking}
          pending={pending}
          error={error}
          onYes={() => void unlink()}
          onNo={() => {
            backToButton.current = true;
            setError(null);
            setConfirm(false);
          }}
        />
      ) : (
        <Button ref={buttonRef} variant="ghost" size="sm" className="text-danger hover:bg-danger-soft" onClick={() => setConfirm(true)}>
          <Unlink aria-hidden="true" />
          {c.unlink}
        </Button>
      )}
    </div>
  );
}
