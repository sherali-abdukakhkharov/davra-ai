import { TrustTag } from "@/components/landing/TrustTag";
import { useFormat } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { Trust } from "@/lib/web";

type Props = {
  id: number;
  createdAt: string;
  author: string;
  trust: Trust;
  body: string;
  kind?: string;
  topic?: string | null;
  replyTo?: number | null;
  addressed?: boolean;
  highlight?: boolean;
  /** One-line snippet instead of the full body. */
  clamp?: boolean;
};

/** One message in the room, in the line shape davra mcp prints: `#id mm-dd hh:mm author [trust]: text`. */
export function MessageLine({ id, createdAt, author, trust, body, kind, topic, replyTo, addressed, highlight, clamp }: Props) {
  const f = useFormat();
  return (
    <div
      id={`m-${id}`}
      className={cn(
        "relative px-4 py-1.5 font-mono text-[0.74rem] leading-[1.75] transition-colors duration-500 sm:px-5",
        highlight && "bg-saffron/15",
      )}
      translate="no"
    >
      <span
        aria-hidden="true"
        className={cn("absolute top-[0.95em] left-1.5 size-1 rounded-full sm:left-2", addressed ? "bg-saffron" : "hidden")}
      />
      <span className="text-fg-muted">#{id}</span>{" "}
      <time dateTime={createdAt} title={f.full(createdAt)} className="tnum text-fg-muted">
        {f.line(createdAt)}
      </time>{" "}
      <span className="font-medium text-fg">{author}</span> <TrustTag trust={trust} tone="ground" />
      {kind && kind !== "chat" ? <span className="text-fg-muted"> ({kind})</span> : null}
      {replyTo ? (
        <a href={`#m-${replyTo}`} className="text-fg-muted underline decoration-dotted underline-offset-2 hover:text-fg">
          {" "}re#{replyTo}
        </a>
      ) : null}
      {topic && topic !== "general" ? <span className="text-accent"> #{topic}</span> : null}
      <span className="text-fg-muted">:</span>{" "}
      <span className={cn("text-fg [overflow-wrap:anywhere]", clamp ? "" : "whitespace-pre-wrap")}>
        {body.split(/(\[redacted\])/i).map((part, k) =>
          /^\[redacted\]$/i.test(part) ? (
            <mark key={k} className="rounded-[3px] bg-bg-sunk px-0.5 text-accent">
              {part}
            </mark>
          ) : (
            <span key={k}>{part}</span>
          ),
        )}
      </span>
    </div>
  );
}
