import React from 'react'
import { createRoot } from 'react-dom/client'
import './styles.css'
import { App } from './App'

// Start in the OS appearance (macOS light/dark); the desktop footer toggles it.
if (window.matchMedia?.('(prefers-color-scheme: dark)').matches) {
  document.documentElement.classList.add('dark')
}

createRoot(document.getElementById('app')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
