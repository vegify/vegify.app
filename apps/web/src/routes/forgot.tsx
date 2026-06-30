import { createFileRoute } from '@tanstack/react-router'
import { ForgotPasswordView } from '@vegify/ui/auth-form'
import { LinkAdapter } from '../link'
import { requestPasswordResetFn } from '../auth'

export const Route = createFileRoute('/forgot')({
  component: ForgotPage,
})

function ForgotPage() {
  return (
    <ForgotPasswordView
      LinkComponent={LinkAdapter}
      onSubmit={async ({ email }) => {
        // Always resolves ok (enumeration-safe) — the view shows the "check your email" confirmation.
        await requestPasswordResetFn({ data: { email } })
      }}
    />
  )
}
