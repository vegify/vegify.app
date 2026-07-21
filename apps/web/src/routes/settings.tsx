import { useQueryClient } from "@tanstack/react-query"
import { createFileRoute, useRouter } from "@tanstack/react-router"
import { createServerFn } from "@tanstack/react-start"
import { type NutritionProfileValues, SettingsView } from "@vegify/ui/screens"

import { deleteAccountFn } from "../auth"

// Settings is a gated page (not in __root's PUBLIC_PATHS) and renders inside the AppShell. The theme
// control is client state (localStorage) inside SettingsView; account deletion (App Review 5.1.1(v))
// and the nutrition profile are the server actions (the session cookie is forwarded as a Bearer).
const runDeleteFn = createServerFn({ method: "POST" })
  .validator((password: string) => password)
  .handler(async ({ data }) => {
    await deleteAccountFn({ data: { password: data } })
  })

// The nutrition profile drives personalized vegan-aware targets in the diary. PRIVATE per-user.
const getProfileFn = createServerFn({ method: "GET" }).handler(
  async (): Promise<NutritionProfileValues> => {
    const { getNutritionProfile } = await import("../content")
    return getNutritionProfile()
  }
)

const saveProfileFn = createServerFn({ method: "POST" })
  .validator((p: NutritionProfileValues) => p)
  .handler(async ({ data }) => {
    const { saveNutritionProfile } = await import("../content")
    await saveNutritionProfile(data)
  })

export const Route = createFileRoute("/settings")({
  loader: () => getProfileFn(),
  component: SettingsPage
})

function SettingsPage() {
  const router = useRouter()
  const queryClient = useQueryClient()
  const profile = Route.useLoaderData()
  return (
    <SettingsView
      profile={profile}
      onSaveProfile={async (values) => {
        await saveProfileFn({ data: values })
      }}
      onDeleteAccount={async (password) => {
        await runDeleteFn({ data: password })
        queryClient.clear()
        await router.invalidate()
        await router.navigate({ to: "/" })
      }}
    />
  )
}
