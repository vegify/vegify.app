import { createFileRoute } from "@tanstack/react-router"
import { ResetPasswordView } from "@vegify/ui/auth-form"

import { confirmPasswordResetFn } from "../auth"
import { LinkAdapter } from "../link"

export const Route = createFileRoute("/reset")({
  // The reset link carries the opaque token as `?token=…`.
  validateSearch: (search: Record<string, unknown>): { token?: string } => ({
    token: typeof search.token === "string" ? search.token : undefined
  }),
  component: ResetPage
})

function ResetPage() {
  const { token } = Route.useSearch()
  return (
    <ResetPasswordView
      token={token ?? ""}
      LinkComponent={LinkAdapter}
      onSubmit={async ({ token, password }) => {
        const res = await confirmPasswordResetFn({ data: { token, password } })
        if (!res.ok) return { error: res.error }
        // success → the view shows the sign-in CTA
      }}
    />
  )
}
