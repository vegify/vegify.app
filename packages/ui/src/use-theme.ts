import { useCallback, useEffect, useState } from "react"

/**
 * Theme system (implemented directly rather than via next-themes —
 * which doesn't integrate cleanly with TanStack Start SSR). System-aware: it follows the OS AND
 * reacts to live appearance changes (matchMedia "change" listener), with a persisted light/dark
 * override. The `.dark` class on <html> is the target (matches @vegify/tokens).
 *
 * No-FOUC: inline `themeScript` in the document <head> (web SSR + desktop index.html) so the class
 * is set before first paint; this hook reconciles after hydration.
 */
export type Theme = "light" | "dark" | "system"

const KEY = "vegify-theme"
const MQ = "(prefers-color-scheme: dark)"

export const themeScript = `(function(){try{var t=localStorage.getItem("${KEY}")||"system";var d=t==="dark"||(t==="system"&&matchMedia("${MQ}").matches);document.documentElement.classList.toggle("dark",d)}catch(e){}})()`

function applyDark(dark: boolean) {
  document.documentElement.classList.toggle("dark", dark)
}

export function useTheme() {
  const [theme, setThemeState] = useState<Theme>("system")

  // Hydrate from storage on mount (server has no localStorage).
  useEffect(() => {
    const stored = localStorage.getItem(KEY) as Theme | null
    if (stored) setThemeState(stored)
  }, [])

  // Apply the class, and while in "system" track live OS appearance changes.
  useEffect(() => {
    const mq = window.matchMedia(MQ)
    applyDark(theme === "dark" || (theme === "system" && mq.matches))
    if (theme !== "system") return
    const onChange = () => applyDark(mq.matches)
    mq.addEventListener("change", onChange)
    return () => mq.removeEventListener("change", onChange)
  }, [theme])

  const setTheme = useCallback((t: Theme) => {
    localStorage.setItem(KEY, t)
    setThemeState(t)
  }, [])

  return { theme, setTheme }
}
