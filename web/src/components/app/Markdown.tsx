import { Fragment, type ReactNode } from "react";

// A small, safe markdown renderer for handoff bodies: builds React elements, never HTML strings.
// Covers what handoff files use: headings, paragraphs, lists, quotes, fenced code, tables (as text),
// inline code, bold, italics and http(s) links.

type Block =
  | { t: "h"; level: number; text: string }
  | { t: "p"; text: string }
  | { t: "ul" | "ol"; items: string[] }
  | { t: "quote"; text: string }
  | { t: "code"; text: string }
  | { t: "hr" };

function parse(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const out: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i]!;
    if (!line.trim()) {
      i++;
      continue;
    }
    if (/^\s*```/.test(line)) {
      const buf: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i]!)) buf.push(lines[i++]!);
      i++;
      out.push({ t: "code", text: buf.join("\n") });
      continue;
    }
    const h = line.match(/^(#{1,6})\s+(.*)$/);
    if (h) {
      out.push({ t: "h", level: h[1]!.length, text: h[2]!.trim() });
      i++;
      continue;
    }
    if (/^\s*(?:-{3,}|\*{3,}|_{3,})\s*$/.test(line)) {
      out.push({ t: "hr" });
      i++;
      continue;
    }
    if (/^\s*\|/.test(line)) {
      const buf: string[] = [];
      while (i < lines.length && /^\s*\|/.test(lines[i]!)) buf.push(lines[i++]!);
      out.push({ t: "code", text: buf.join("\n") });
      continue;
    }
    if (/^\s*>/.test(line)) {
      const buf: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i]!)) buf.push(lines[i++]!.replace(/^\s*>\s?/, ""));
      out.push({ t: "quote", text: buf.join(" ") });
      continue;
    }
    const ul = /^\s*[-*+]\s+/;
    const ol = /^\s*\d+[.)]\s+/;
    if (ul.test(line) || ol.test(line)) {
      const kind = ul.test(line) ? "ul" : "ol";
      const re = kind === "ul" ? ul : ol;
      const items: string[] = [];
      while (i < lines.length && lines[i]!.trim()) {
        const l = lines[i]!;
        if (re.test(l)) items.push(l.replace(re, ""));
        else if (items.length) items[items.length - 1] += " " + l.trim();
        i++;
      }
      out.push({ t: kind, items });
      continue;
    }
    const buf: string[] = [];
    while (
      i < lines.length &&
      lines[i]!.trim() &&
      !/^(#{1,6})\s/.test(lines[i]!) &&
      !/^\s*```/.test(lines[i]!) &&
      !ul.test(lines[i]!) &&
      !ol.test(lines[i]!) &&
      !/^\s*[>|]/.test(lines[i]!)
    )
      buf.push(lines[i++]!);
    out.push({ t: "p", text: buf.join("\n") });
  }
  return out;
}

const INLINE = /(`[^`]+`)|(\*\*[^*]+\*\*|__[^_]+__)|(\[[^\]]+\]\((https?:\/\/[^)\s]+)\))|(\*[^*\s][^*]*\*|_[^_\s][^_]*_)/g;

function inline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  let k = 0;
  for (const m of text.matchAll(INLINE)) {
    const at = m.index ?? 0;
    if (at > last) out.push(text.slice(last, at));
    const s = m[0];
    if (m[1]) out.push(<code key={k++} className="rounded-[3px] bg-bg-sunk px-1 py-px font-mono text-[0.85em]">{s.slice(1, -1)}</code>);
    else if (m[2]) out.push(<strong key={k++} className="font-semibold">{inline(s.slice(2, -2))}</strong>);
    else if (m[3]) {
      const label = s.slice(1, s.indexOf("]("));
      out.push(
        <a key={k++} href={m[4]} target="_blank" rel="noreferrer noopener" className="text-accent underline decoration-current/40 hover:decoration-current">
          {label}
        </a>,
      );
    } else if (m[5]) out.push(<em key={k++}>{s.slice(1, -1)}</em>);
    last = at + s.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

function lines(text: string) {
  return text.split("\n").map((l, i, a) => (
    <Fragment key={i}>
      {inline(l)}
      {i < a.length - 1 ? <br /> : null}
    </Fragment>
  ));
}

export function Markdown({ source }: { source: string }) {
  const blocks = parse(source);
  return (
    <div className="flex max-w-[72ch] flex-col gap-3 text-[0.95rem] leading-relaxed [overflow-wrap:anywhere]">
      {blocks.map((b, i) => {
        switch (b.t) {
          case "h": {
            const size = b.level <= 2 ? "text-[1.05rem]" : "text-[0.95rem]";
            return (
              <p key={i} role="heading" aria-level={Math.min(6, b.level + 2)} className={`${size} mt-2 font-semibold first:mt-0`}>
                {inline(b.text)}
              </p>
            );
          }
          case "p":
            return <p key={i}>{lines(b.text)}</p>;
          case "ul":
            return (
              <ul key={i} className="list-disc space-y-1 pl-5 marker:text-fg-muted">
                {b.items.map((x, j) => (
                  <li key={j}>{inline(x)}</li>
                ))}
              </ul>
            );
          case "ol":
            return (
              <ol key={i} className="list-decimal space-y-1 pl-5 marker:text-fg-muted">
                {b.items.map((x, j) => (
                  <li key={j}>{inline(x)}</li>
                ))}
              </ol>
            );
          case "quote":
            return (
              <blockquote key={i} className="border-l border-rule-strong pl-4 text-fg-muted">
                {inline(b.text)}
              </blockquote>
            );
          case "code":
            return (
              <pre key={i} translate="no" className="overflow-x-auto rounded-md border border-rule bg-bg-sunk px-3.5 py-2.5 font-mono text-[0.75rem] leading-[1.7] [scrollbar-width:thin]">
                <code>{b.text}</code>
              </pre>
            );
          case "hr":
            return <hr key={i} className="border-rule" />;
        }
      })}
    </div>
  );
}
