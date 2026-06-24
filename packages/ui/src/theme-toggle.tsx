import { useEffect, useState } from "react";
import { useTheme } from "next-themes";
import { Monitor, Moon, Sun } from "lucide-react";
import { cn } from "./cn";

/**
 * Shared theme control for both shells. Backed by next-themes, so it's system-aware (follows the
 * OS AND reacts to OS appearance changes live) and persists an explicit Light/Dark override.
 * Cycles System → Light → Dark. SSR-safe: renders a stable placeholder until mounted so the
 * server HTML (which can't know the client's theme) and the first client render agree.
 */
const ORDER = ["system", "light", "dark"] as const;
const META: Record<(typeof ORDER)[number], { icon: typeof Monitor; label: string }> = {
  system: { icon: Monitor, label: "System" },
  light: { icon: Sun, label: "Light" },
  dark: { icon: Moon, label: "Dark" },
};

export function ThemeToggle({ className }: { className?: string }) {
  const { theme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  const current = (mounted && (theme as (typeof ORDER)[number] | undefined)) || "system";
  const { icon: Icon, label } = META[current] ?? META.system;
  const cycle = () => setTheme(ORDER[(ORDER.indexOf(current) + 1) % ORDER.length]);

  return (
    <button
      type="button"
      onClick={cycle}
      aria-label={`Theme: ${label} (click to change)`}
      className={cn(
        "flex w-full items-center justify-center gap-2 rounded-lg bg-white/10 px-3 py-2 text-sm font-medium text-white transition hover:bg-white/20",
        className,
      )}
    >
      <Icon className="size-4" />
      {label}
    </button>
  );
}
