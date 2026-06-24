import React from 'react'
import { createRoot } from 'react-dom/client'
import './styles.css'
import { App } from './App'

// Theme is applied before paint by the inline script in index.html (themeScript); the shared
// ThemeToggle / useTheme (@vegify/ui) manage it after mount.
createRoot(document.getElementById('app')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
