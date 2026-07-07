import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// Minimal Vite + React frontend for the Tauri desktop shell. No TanStack Start / router —
// this shell is client-rendered and offline-first; data comes from the Rust backend over
// tauri-typed-ipc (see src-tauri), not a server.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Tauri expects a fixed dev port and shouldn't have the screen cleared.
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "es2022" }
})
