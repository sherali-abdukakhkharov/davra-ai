import { ArrowUp, LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { Button } from "@/components/ui/button";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import { canPost, web, webErrorText, type Message, type ProjectRef } from "@/lib/web";
import { MessageLine } from "./MessageLine";
import { Notice } from "./States";

const POLL_MS = 15_000;

function merge(a: Message[], b: Message[]): Message[] {
  const byId = new Map<number, Message>();
  for (const m of a) byId.set(m.id, m);
  for (const m of b) byId.set(m.id, m);
  return [...byId.values()].sort((x, y) => x.id - y.id);
}

export function RoomTab({ project, initial, onPosted }: { project: ProjectRef; initial: Message[]; onPosted: () => void }) {
  const { t } = usePrefs();
  const r = t.app.room;
  const [messages, setMessages] = useState<Message[]>(initial);
  const [older, setOlder] = useState<"idle" | "loading" | "done" | { error: string }>(initial.length < 30 ? "done" : "idle");
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [fresh, setFresh] = useState<number | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const keepBottom = useRef(true);
  const restore = useRef<number | null>(null);
  const textRef = useRef<HTMLTextAreaElement>(null);
  const { org, slug } = project;

  // Stick to the newest line unless the reader has scrolled up; keep position when older lines are prepended.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    if (restore.current !== null) {
      el.scrollTop = el.scrollHeight - restore.current;
      restore.current = null;
    } else if (keepBottom.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [messages]);

  const onScroll = () => {
    const el = scroller.current;
    if (el) keepBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
  };

  const refresh = useCallback(async () => {
    try {
      const d = await web.project(org, slug);
      setMessages((m) => merge(m, d.messages));
    } catch {
      // Quiet background refresh; the next tick tries again.
    }
  }, [org, slug]);

  useEffect(() => {
    const id = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, POLL_MS);
    return () => window.clearInterval(id);
  }, [refresh]);

  const loadOlder = async () => {
    const first = messages[0];
    if (!first) return;
    setOlder("loading");
    try {
      const { messages: got } = await web.older(org, slug, first.id);
      const el = scroller.current;
      if (el) restore.current = el.scrollHeight - el.scrollTop;
      setMessages((m) => merge(got, m));
      setOlder(got.length < 30 ? "done" : "idle");
    } catch (err) {
      setOlder({ error: webErrorText(err, t.app.errors) });
    }
  };

  const send = async (ev?: FormEvent) => {
    ev?.preventDefault();
    const body = text.trim();
    if (sending) return;
    if (!body) {
      setSendError(r.required);
      textRef.current?.focus();
      return;
    }
    setSending(true);
    setSendError(null);
    setNotice(null);
    try {
      const res = await web.post(org, slug, body);
      setText("");
      keepBottom.current = true;
      await refresh();
      setFresh(res.id);
      window.setTimeout(() => setFresh(null), 2400);
      if (res.redacted) setNotice(r.redacted);
      else if (!res.relayed && project.telegram) setNotice(r.notRelayed);
      onPosted();
    } catch (err) {
      setSendError(webErrorText(err, t.app.errors));
    } finally {
      setSending(false);
      textRef.current?.focus();
    }
  };

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void send();
    }
  };

  const me = project.handle;
  const allowed = canPost(project.role);

  return (
    <div className="flex flex-col gap-4">
      <div className="overflow-hidden rounded-lg border border-rule bg-bg-raised">
        <div
          ref={scroller}
          onScroll={onScroll}
          className="flex h-[min(62dvh,40rem)] min-h-[18rem] flex-col overflow-y-auto overscroll-contain [scrollbar-width:thin]"
          aria-label={t.app.project.tabs.room}
          role="log"
          aria-live="polite"
          aria-relevant="additions"
          tabIndex={0}
        >
          <div className="flex justify-center border-b border-rule py-2.5">
            {older === "done" ? (
              <span className="text-xs text-fg-muted">{messages.length ? r.noOlder : ""}</span>
            ) : (
              <Button variant="ghost" size="sm" onClick={loadOlder} disabled={older === "loading"}>
                {older === "loading" ? <LoaderCircle className="animate-spin motion-reduce:animate-none" aria-hidden="true" /> : <ArrowUp aria-hidden="true" />}
                {older === "loading" ? r.loadingOlder : r.loadOlder}
              </Button>
            )}
          </div>
          {typeof older === "object" ? (
            <p role="alert" className="px-5 py-2 text-sm text-danger">
              {older.error}
            </p>
          ) : null}
          {messages.length === 0 ? (
            <p className="my-auto px-5 py-10 text-center text-fg-muted">{r.empty}</p>
          ) : (
            <ol className="mt-auto py-2">
              {messages.map((m) => (
                <li key={m.id}>
                  <MessageLine
                    id={m.id}
                    createdAt={m.created_at}
                    author={m.author}
                    trust={m.trust}
                    body={m.body}
                    kind={m.kind}
                    topic={m.topic}
                    replyTo={m.reply_to}
                    addressed={m.mentions.includes(me)}
                    highlight={m.id === fresh}
                  />
                </li>
              ))}
            </ol>
          )}
        </div>
      </div>

      <p className="text-xs text-fg-muted">{r.trustNote}</p>

      {allowed ? (
        <form onSubmit={send} className="flex flex-col gap-2">
          <label htmlFor="composer" className="sr-only">
            {r.composer}
          </label>
          <div
            className={cn(
              "flex items-end gap-2 rounded-lg border bg-bg-raised p-2 transition-[border-color,box-shadow] focus-within:border-accent focus-within:ring-3 focus-within:ring-ring/25",
              sendError ? "border-danger" : "border-rule-strong",
            )}
          >
            <textarea
              id="composer"
              ref={textRef}
              name="text"
              rows={2}
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                if (sendError) setSendError(null);
              }}
              onKeyDown={onKey}
              placeholder={r.placeholder}
              aria-invalid={sendError ? true : undefined}
              aria-describedby="composer-hint"
              className="max-h-48 min-h-[3rem] flex-1 resize-y bg-transparent px-2 py-1.5 text-[0.95rem] outline-none placeholder:text-fg-muted/80 [field-sizing:content]"
            />
            <Button type="submit" disabled={sending} className="shrink-0">
              {sending ? <LoaderCircle className="animate-spin motion-reduce:animate-none" aria-hidden="true" /> : null}
              {sending ? r.sending : r.send}
            </Button>
          </div>
          <div id="composer-hint" className="flex flex-wrap justify-between gap-x-4 gap-y-1 text-xs text-fg-muted">
            <span>{fill(project.telegram ? r.hint : r.hintNoTg, { handle: me })}</span>
            <kbd className="hidden font-sans sm:inline">{r.shortcut}</kbd>
          </div>
          {sendError ? (
            <p role="alert" className="text-sm text-danger">
              {sendError}
            </p>
          ) : null}
          {notice ? <Notice tone="warn">{notice}</Notice> : null}
        </form>
      ) : (
        <Notice>{r.viewer}</Notice>
      )}
    </div>
  );
}
