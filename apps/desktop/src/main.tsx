import React from "react"
import { takeoverConsole } from "@fltsci/tauri-plugin-tracing"
import { createRoot } from "react-dom/client"
import "./styles.css"
import { App } from "./App"

// Unify logging (fltsci tauri-plugin-tracing): route the frontend console — log/info/warn/error/… —
// through Rust tracing, and surface Rust logs back in the webview devtools (JS → Rust → console), so
// the UI's logs join the same structured stream as the sync runtime's spans. Fire-and-forget; the
// invoke needs the `tracing:default` capability (granted in capabilities/desktop.json — DESKTOP
// only: the plugin doesn't compile for mobile, so on iOS this rejects and the catch keeps it quiet).
void takeoverConsole().catch(() => {})

// Theme is applied before paint by the inline script in index.html (themeScript); the shared
// useTheme (@vegify/ui) — surfaced as the Theme control in Settings — manages it after mount.
const appRoot = document.getElementById("app")
if (!appRoot) throw new Error("#app missing from index.html")
createRoot(appRoot).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
)
