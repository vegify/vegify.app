import { createFileRoute } from '@tanstack/react-router'
import { SettingsView } from '@vegify/ui'

// Settings is a gated page (not in __root's PUBLIC_PATHS) and renders inside the AppShell. The theme
// control is client state (localStorage) inside SettingsView, so the route needs no loader.
export const Route = createFileRoute('/settings')({
  component: SettingsView,
})
