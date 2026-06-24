import { useEffect, useState } from "react";
import { Monitor, Moon, Sun } from "lucide-react";
import { cn } from "./cn";
import { useTheme, type Theme } from "./use-theme";

/**
 * Shared theme control for both shells (rendered by AppShell). Cycles System → Light → Dark.
 * Backed by the system-aware useTheme (reacts to live OS changes). SSR-safe: a mounted guard keeps
 * the server HTML and first client render in agreement (both show "System") to avoid a mismatch.
 */
const ORDER: Theme[] = ["system", "light", "dark"];
const META: Record<Theme, { icon: typeof Monitor; label: string }> = {
  system: { icon: Monitor, label: "System" },
  light: { icon: Sun, label: "Light" },
  dark: { icon: Moon, label: "Dark" },
};

export function ThemeToggle({ className }: { className?: string }) {
  const { theme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  const current: Theme = mounted ? theme : "system";
  const { icon: Icon, label } = META[current];
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
