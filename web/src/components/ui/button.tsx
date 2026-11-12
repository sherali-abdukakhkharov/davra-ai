import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

// shadcn/ui button, restyled for the Davra tokens.
const buttonVariants = cva(
  "inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-md font-medium transition-[background-color,color,box-shadow,transform] duration-200 ease-out select-none touch-manipulation disabled:pointer-events-none disabled:opacity-60 active:translate-y-px [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        primary: "bg-primary text-primary-fg hover:bg-primary-hover",
        saffron:
          "bg-saffron text-[oklch(0.22_0.08_268)] hover:bg-[oklch(0.86_0.13_80)] shadow-[0_1px_0_oklch(1_0_0/0.35)_inset,0_6px_18px_-8px_oklch(0.15_0.1_268/0.8)]",
        outline: "border border-rule-strong bg-transparent text-fg hover:bg-bg-sunk",
        fieldOutline: "border border-field-rule bg-transparent text-field-fg hover:bg-field-deep hover:border-field-muted",
        ghost: "bg-transparent text-fg hover:bg-bg-sunk",
        fieldGhost: "bg-transparent text-field-fg hover:bg-field-deep",
        link: "h-auto rounded-sm px-0 text-accent underline decoration-current/40 hover:decoration-current",
      },
      size: {
        sm: "h-9 px-3 text-sm",
        md: "h-11 px-5 text-[0.95rem]",
        lg: "h-12 px-6 text-base",
        icon: "size-10",
      },
    },
    defaultVariants: { variant: "primary", size: "md" },
  },
);

export type ButtonProps = ComponentProps<"button"> & VariantProps<typeof buttonVariants> & { asChild?: boolean };

export function Button({ className, variant, size, asChild = false, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : "button";
  return <Comp data-slot="button" className={cn(buttonVariants({ variant, size, className }))} {...props} />;
}

export { buttonVariants };
