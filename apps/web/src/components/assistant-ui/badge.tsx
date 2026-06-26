"use client";

import type { ComponentProps } from "react";
import { Slot } from "radix-ui";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center justify-center gap-1 rounded-md text-xs font-medium transition-colors [&_svg]:size-3 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        outline:
          "border-input text-muted-foreground hover:bg-accent hover:text-accent-foreground border bg-transparent",
        secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/80",
        muted: "bg-muted text-muted-foreground hover:bg-muted/80 hover:text-foreground",
        ghost: "text-muted-foreground hover:bg-accent hover:text-accent-foreground bg-transparent",
        info: "bg-info/15 text-info hover:bg-info/20",
        warning: "bg-warning/15 text-warning hover:bg-warning/20",
        success: "bg-success/15 text-success hover:bg-success/20",
        destructive: "bg-destructive/15 text-destructive hover:bg-destructive/20",
      },
      size: {
        sm: "px-1.5 py-0.5",
        default: "px-2 py-1",
        lg: "px-2.5 py-1.5 text-sm",
      },
    },
    defaultVariants: {
      variant: "outline",
      size: "default",
    },
  },
);

export type BadgeProps = ComponentProps<"span"> &
  VariantProps<typeof badgeVariants> & {
    asChild?: boolean;
  };

function Badge({ className, variant, size, asChild = false, ...props }: BadgeProps) {
  const Comp = asChild ? Slot.Root : "span";

  return (
    <Comp
      data-slot="badge"
      data-variant={variant}
      data-size={size}
      className={cn(badgeVariants({ variant, size }), className)}
      {...props}
    />
  );
}

export { Badge, badgeVariants };
