import { cn } from "@/lib/utils";
import type { Trust } from "./room";

/**
 * Trust label as an agent sees it. Filled = may instruct; outlined = information.
 * `tone` picks the palette for the cobalt field or the porcelain ground.
 */
export function TrustTag({ trust, tone = "field", className }: { trust: Trust; tone?: "field" | "ground"; className?: string }) {
  const field = tone === "field";
  const styles: Record<Trust, string> = {
    owner: "bg-saffron text-[oklch(0.22_0.08_268)] border-saffron",
    self: field ? "bg-glaze text-[oklch(0.2_0.07_268)] border-glaze" : "bg-accent text-bg border-accent",
    peer_human: field ? "border-field-muted/70 text-field-fg" : "border-rule-strong text-fg",
    peer_agent: field ? "border-field-muted/70 text-field-fg" : "border-rule-strong text-fg",
    external: field
      ? "border-dashed border-field-muted/70 text-field-muted"
      : "border-dashed border-fg-muted text-fg-muted",
  };
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-[4px] border px-1.5 py-px font-mono text-[0.68rem] leading-[1.35] font-medium whitespace-nowrap transition-colors duration-300",
        styles[trust],
        className,
      )}
    >
      {trust}
    </span>
  );
}
