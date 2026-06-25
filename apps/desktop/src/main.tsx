import React from 'react'
import { createRoot } from 'react-dom/client'
import { takeoverConsole } from '@fltsci/tauri-plugin-tracing'
import './styles.css'
import { App } from './App'

// Unify logging (fltsci tauri-plugin-tracing): route the frontend console — log/info/warn/error/… —
// through Rust tracing, and surface Rust logs back in the webview devtools (JS → Rust → console), so
// the UI's logs join the same structured stream as the sync runtime's spans. Fire-and-forget; the
// invoke needs the `tracing:default` capability (granted in capabilities/default.json).
void takeoverConsole().catch(() => {})

// Theme is applied before paint by the inline script in index.html (themeScript); the shared
// ThemeToggle / useTheme (@vegify/ui) manage it after mount.
createRoot(document.getElementById('app')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
