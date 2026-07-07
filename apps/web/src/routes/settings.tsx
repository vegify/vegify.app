import { useQueryClient } from "@tanstack/react-query"
import { createFileRoute, useRouter } from "@tanstack/react-router"
import { createServerFn } from "@tanstack/react-start"
import { SettingsView } from "@vegify/ui/screens"

import { deleteAccountFn } from "../auth"

// Settings is a gated page (not in __root's PUBLIC_PATHS) and renders inside the AppShell. The theme
// control is client state (localStorage) inside SettingsView; account deletion (App Review 5.1.1(v))
// is the one server action.
const runDeleteFn = createServerFn({ method: "POST" })
  .validator((password: string) => password)
  .handler(async ({ data }) => {
    await deleteAccountFn({ data: { password: data } })
  })

export const Route = createFileRoute("/settings")({
  component: SettingsPage
})

function SettingsPage() {
  const router = useRouter()
  const queryClient = useQueryClient()
  return (
    <SettingsView
      onDeleteAccount={async (password) => {
        await runDeleteFn({ data: password })
        queryClient.clear()
        await router.invalidate()
        await router.navigate({ to: "/" })
      }}
    />
  )
}
