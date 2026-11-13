import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

// shadcn/ui input, restyled for the Davra tokens.
export function Input({ className, type, ...props }: ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "h-12 w-full min-w-0 rounded-md border border-rule-strong bg-bg-raised px-3.5 text-base text-fg transition-[border-color,box-shadow] duration-150 outline-none",
        "placeholder:text-fg-muted/80 hover:border-fg-muted/60",
        "focus-visible:border-accent focus-visible:ring-3 focus-visible:ring-ring/30 focus-visible:outline-none",
        "aria-invalid:border-danger aria-invalid:focus-visible:ring-danger/25",
        "disabled:cursor-not-allowed disabled:opacity-60",
        className,
      )}
      {...props}
    />
  );
}
