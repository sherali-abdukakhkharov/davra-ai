// @mention helpers for the room composer. Handles follow Telegram usernames: a–z 0–9 _ - .
// (a trailing "." ends the handle, as at the end of a sentence).

import type { Member } from "@/lib/web";

/** The "@query" being typed at the caret, or null when the caret is not in a mention at a word start. */
export function activeMention(text: string, caret: number): { start: number; query: string } | null {
  const before = text.slice(0, caret);
  // "@" must start a word: at the beginning or after whitespace / opening punctuation, so "a@b.uz" never opens.
  const m = before.match(/(^|[\s([{"'«“])@([\p{L}\p{N}_.-]*)$/u);
  if (!m) return null;
  const query = m[2]!;
  if (query.length > 64) return null;
  return { start: caret - query.length - 1, query };
}

const fold = (s: string) =>
  s
    .normalize("NFKD")
    .replace(/[̀-ͯʻʼ’']/g, "")
    .toLowerCase();

/** Members matching the typed prefix by handle or by any word of the display name. Excludes the viewer. */
export function suggest(members: Member[], query: string, limit = 8): Member[] {
  const q = fold(query.replace(/\.+$/, ""));
  const pool = members.filter((m) => !m.is_me);
  if (!q) return pool.slice(0, limit);
  const byHandle: Member[] = [];
  const byName: Member[] = [];
  for (const m of pool) {
    if (fold(m.handle).startsWith(q)) byHandle.push(m);
    else if (m.display_name && fold(m.display_name).split(/[\s_.-]+/).some((w) => w.startsWith(q))) byName.push(m);
  }
  return [...byHandle, ...byName].slice(0, limit);
}

/** A finished handle followed by "." reads as the end of a sentence, so the picker should close. */
export function endsSentence(members: Member[], query: string): boolean {
  if (!query.endsWith(".")) return false;
  const h = query.replace(/\.+$/, "").toLowerCase();
  return members.some((m) => m.handle.toLowerCase() === h);
}

const MIRROR_PROPS = [
  "boxSizing", "width", "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth",
  "paddingTop", "paddingRight", "paddingBottom", "paddingLeft", "fontFamily", "fontSize", "fontWeight",
  "fontStyle", "fontVariant", "fontStretch", "lineHeight", "letterSpacing", "wordSpacing", "textTransform",
  "textIndent", "tabSize",
] as const;

/** Caret position inside a textarea, in px from the textarea's top-left (scroll accounted for). */
export function caretCoords(el: HTMLTextAreaElement, index: number): { top: number; left: number; height: number } {
  const cs = window.getComputedStyle(el);
  const div = document.createElement("div");
  const st = div.style;
  for (const p of MIRROR_PROPS) st[p] = cs[p];
  st.position = "absolute";
  st.visibility = "hidden";
  st.whiteSpace = "pre-wrap";
  st.overflowWrap = "break-word";
  st.top = "0";
  st.left = "-9999px";
  div.textContent = el.value.slice(0, index);
  const mark = document.createElement("span");
  mark.textContent = el.value.slice(index) || ".";
  div.appendChild(mark);
  document.body.appendChild(div);
  const lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.4;
  const out = { top: mark.offsetTop - el.scrollTop, left: mark.offsetLeft - el.scrollLeft, height: lh };
  div.remove();
  return out;
}
