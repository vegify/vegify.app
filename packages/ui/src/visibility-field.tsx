"use client";

import { cn } from "./cn";

// Mirrors @vegify/db's Visibility (the package boundary keeps @vegify/ui independent of @vegify/db).
export type Visibility = "public" | "private" | "unlisted";

const OPTIONS: { value: Visibility; label: string; hint: string }[] = [
  { value: "public", label: "Public", hint: "Anyone can find and view it" },
  { value: "unlisted", label: "Unlisted", hint: "Only people with the link" },
  { value: "private", label: "Private", hint: "Only you" },
];

/** Segmented control for UGC visibility (public-default sharing). */
export function VisibilityField({
  value,
  onChange,
}: {
  value: Visibility;
  onChange: (v: Visibility) => void;
}) {
  return (
    <div>
      <div className="flex gap-1 rounded-lg bg-muted p-1">
        {OPTIONS.map((o) => (
          <button
            key={o.value}
            type="button"
            onClick={() => onChange(o.value)}
            aria-pressed={value === o.value}
            className={cn(
              "flex-1 rounded-md px-3 py-1.5 font-medium text-sm transition",
              value === o.value
                ? "bg-card text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {o.label}
          </button>
        ))}
      </div>
      <p className="mt-1 text-center text-muted-foreground text-xs">
        {OPTIONS.find((o) => o.value === value)?.hint}
      </p>
    </div>
  );
}
