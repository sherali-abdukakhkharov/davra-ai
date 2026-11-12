import { useCallback, useLayoutEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import { fill, usePrefs } from "@/lib/prefs";
import { cn } from "@/lib/utils";
import type { Member } from "@/lib/web";
import { activeMention, caretCoords, endsSentence, suggest } from "./mention";
import { Seat } from "./MembersTab";

type Props = {
  id: string;
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  members: Member[];
  placeholder: string;
  invalid: boolean;
  describedBy: string;
  textRef: RefObject<HTMLTextAreaElement | null>;
};

type Open = { start: number; query: string; items: Member[]; active: number };

const LIST_ID = "mention-list";
const PICKER_W = 288; // 18rem

/**
 * The room's message box with a Telegram-style @mention picker (ARIA combobox + listbox).
 * ↑/↓ move, Enter or Tab insert "@handle ", Esc closes, Ctrl/Cmd+Enter still sends.
 */
export function Composer({ id, value, onChange, onSend, members, placeholder, invalid, describedBy, textRef }: Props) {
  const { t } = usePrefs();
  const m = t.app.mention;
  const [open, setOpen] = useState<Open | null>(null);
  const [pos, setPos] = useState<{ bottom: number; left: number; width: number } | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const pendingCaret = useRef<number | null>(null);

  const evaluate = useCallback(
    (el: HTMLTextAreaElement) => {
      if (el.selectionStart !== el.selectionEnd) return setOpen(null);
      const hit = activeMention(el.value, el.selectionStart);
      if (!hit || endsSentence(members, hit.query)) return setOpen(null);
      const items = suggest(members, hit.query);
      if (!items.length) return setOpen(null);
      setOpen((prev) => ({
        ...hit,
        items,
        active: prev && prev.start === hit.start ? Math.min(prev.active, items.length - 1) : 0,
      }));
    },
    [members],
  );

  // Place the list above the caret line, clamped inside the composer (and so inside a 360px viewport).
  useLayoutEffect(() => {
    const el = textRef.current;
    const wrap = wrapRef.current;
    if (!open || !el || !wrap) return setPos(null);
    const c = caretCoords(el, open.start);
    const wr = wrap.getBoundingClientRect();
    const er = el.getBoundingClientRect();
    // The list may use the whole composer row (textarea + Send), not only the textarea's width.
    const row = (wrap.parentElement ?? wrap).getBoundingClientRect();
    const caretTop = er.top - wr.top + c.top;
    const width = Math.min(PICKER_W, row.width - 8);
    const minLeft = row.left - wr.left + 4;
    const maxLeft = row.right - wr.left - width - 4;
    const left = Math.max(minLeft, Math.min(er.left - wr.left + c.left - 12, maxLeft));
    setPos({ bottom: wr.height - caretTop + 6, left, width });
  }, [open, textRef, value]);

  // Restore the caret after inserting a handle.
  useLayoutEffect(() => {
    const el = textRef.current;
    if (el && pendingCaret.current !== null) {
      el.setSelectionRange(pendingCaret.current, pendingCaret.current);
      pendingCaret.current = null;
    }
  }, [value, textRef]);

  const insert = (member: Member) => {
    const el = textRef.current;
    if (!el || !open) return;
    const caret = el.selectionStart;
    const after = value.slice(caret);
    const spacer = /^\s/.test(after) ? "" : " ";
    const next = `${value.slice(0, open.start)}@${member.handle}${spacer}${after}`;
    // Caret goes after "@handle" and the space that follows it.
    pendingCaret.current = open.start + member.handle.length + 2;
    onChange(next);
    setOpen(null);
    el.focus();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      setOpen(null);
      onSend();
      return;
    }
    if (!open) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const d = e.key === "ArrowDown" ? 1 : -1;
      setOpen({ ...open, active: (open.active + d + open.items.length) % open.items.length });
    } else if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      const pick = open.items[open.active];
      if (pick) insert(pick);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      setOpen(null);
    }
  };

  const activeId = open ? `mention-${open.items[open.active]?.handle}` : undefined;

  return (
    <div ref={wrapRef} className="relative min-w-0 flex-1">
      <textarea
        id={id}
        ref={textRef}
        name="text"
        rows={2}
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          evaluate(e.target);
        }}
        onKeyDown={onKeyDown}
        onKeyUp={(e) => {
          if (e.key === "ArrowLeft" || e.key === "ArrowRight" || e.key === "Home" || e.key === "End") evaluate(e.currentTarget);
        }}
        onClick={(e) => evaluate(e.currentTarget)}
        onBlur={() => setOpen(null)}
        placeholder={placeholder}
        role="combobox"
        aria-expanded={open ? true : false}
        aria-controls={LIST_ID}
        aria-autocomplete="list"
        aria-haspopup="listbox"
        aria-activedescendant={activeId}
        aria-invalid={invalid ? true : undefined}
        aria-describedby={describedBy}
        autoComplete="off"
        className="block max-h-48 min-h-[3rem] w-full resize-y bg-transparent px-2 py-1.5 text-[0.95rem] leading-normal outline-none placeholder:text-fg-muted/80 [field-sizing:content]"
      />
      <ul
        id={LIST_ID}
        role="listbox"
        aria-label={m.listLabel}
        hidden={!open || !pos}
        style={pos ? { bottom: pos.bottom, left: pos.left, width: pos.width } : undefined}
        className="absolute z-20 max-h-64 overflow-y-auto overscroll-contain rounded-lg border border-rule-strong bg-bg-raised py-1 shadow-[0_16px_40px_-12px_hsl(var(--shadow-color)/0.45)] [scrollbar-width:thin]"
      >
        {open?.items.map((x, i) => {
          const active = i === open.active;
          return (
            <li
              key={x.handle}
              id={`mention-${x.handle}`}
              role="option"
              aria-selected={active}
              // Keep focus in the textarea so the caret survives a mouse or touch pick.
              onMouseDown={(e) => e.preventDefault()}
              onMouseMove={() => !active && setOpen({ ...open, active: i })}
              onClick={() => insert(x)}
              className={cn(
                "flex cursor-pointer items-center gap-3 px-3 py-2 select-none touch-manipulation",
                active ? "bg-bg-sunk" : "",
              )}
            >
              <Seat kind={x.kind} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium text-fg">{x.display_name || x.handle}</span>
                <span className="block truncate font-mono text-[0.72rem] text-fg-muted" translate="no">
                  @{x.handle}
                  {x.kind === "agent" && x.owner ? <span className="font-sans"> · {fill(m.agentOf, { owner: x.owner })}</span> : null}
                </span>
              </span>
              <span
                className={cn(
                  "shrink-0 rounded-[4px] border px-1.5 py-px text-[0.66rem]",
                  x.kind === "agent" ? "border-accent text-accent" : "border-rule-strong text-fg-muted",
                )}
              >
                {x.kind === "agent" ? m.agent : m.human}
              </span>
            </li>
          );
        })}
      </ul>
      <span className="sr-only" role="status" aria-live="polite">
        {open ? fill(m.count, { n: open.items.length }) : ""}
      </span>
    </div>
  );
}
