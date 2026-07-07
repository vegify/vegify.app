import { Monitor, Moon, Sun } from "lucide-react";
import { useEffect, useState } from "react";
import { cn } from "./cn";
import { type Theme, useTheme } from "./use-theme";

/**
 * Theme setting — the System / Light / Dark control shown on the Settings screen. A segmented
 * control (one option highlighted) rather than the compact cycle button, since a settings page
 * shows every choice explicitly. Backed by the system-aware useTheme (follows the OS AND reacts to
 * live appearance changes). Styled with theme tokens so it sits on the content background (the old
 * sidebar toggle was white-on-green). SSR-safe: a mounted guard keeps the server HTML and first
 * client render in agreement (both show "System") to avoid a hydration mismatch.
 */
const OPTIONS: { value: Theme; icon: typeof Monitor; label: string }[] = [
  { value: "system", icon: Monitor, label: "System" },
  { value: "light", icon: Sun, label: "Light" },
  { value: "dark", icon: Moon, label: "Dark" },
];

export function ThemeSetting({ className }: { className?: string }) {
  const { theme, setTheme } = useTheme();
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);
  const current: Theme = mounted ? theme : "system";

  return (
    <fieldset
      aria-label="Theme"
      className={cn(
        "inline-flex items-center gap-1 rounded-xl bg-muted p-1 ring-1 ring-foreground/10",
        className,
      )}
    >
      {OPTIONS.map(({ value, icon: Icon, label }) => {
        const active = current === value;
        return (
          <button
            key={value}
            type="button"
            aria-pressed={active}
            onClick={() => setTheme(value)}
            className={cn(
              "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium transition",
              active
                ? "bg-card text-foreground shadow-sm ring-1 ring-foreground/10"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            <Icon className="size-4" />
            {label}
          </button>
        );
      })}
    </fieldset>
  );
}
