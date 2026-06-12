import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";
import { cn } from "./cn";

// Placeholder primitives until the shadcn (Base UI) pass — same API shape so the
// swap is a drop-in: cva variants + className passthrough.
const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 rounded-full font-semibold uppercase tracking-wide transition-colors disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        primary: "bg-primary text-white hover:bg-primary-dark",
        accent: "bg-accent text-gray-900 hover:bg-yellow",
        outline:
          "border border-gray-500 text-gray-900 hover:border-primary hover:text-primary",
      },
      size: {
        sm: "h-8 px-4 text-xs",
        md: "h-10 px-6 text-sm",
        lg: "h-12 px-8 text-base",
      },
    },
    defaultVariants: { variant: "primary", size: "md" },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export function Button({ className, variant, size, ...props }: ButtonProps) {
  return (
    <button className={cn(buttonVariants({ variant, size }), className)} {...props} />
  );
}

export function buttonClasses(
  opts?: VariantProps<typeof buttonVariants> & { className?: string }
) {
  const { className, ...variants } = opts ?? {};
  return cn(buttonVariants(variants), className);
}
