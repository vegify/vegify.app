import { createFileRoute } from '@tanstack/react-router'
import { VerifyEmailView } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { confirmEmailVerificationFn } from '../auth'

export const Route = createFileRoute('/verify')({
  // The verification link carries the opaque token as `?token=…`.
  validateSearch: (search: Record<string, unknown>): { token?: string } => ({
    token: typeof search.token === 'string' ? search.token : undefined,
  }),
  component: VerifyPage,
})

function VerifyPage() {
  const { token } = Route.useSearch()
  return (
    <VerifyEmailView
      token={token ?? ''}
      LinkComponent={LinkAdapter}
      onSubmit={async ({ token }) => {
        const res = await confirmEmailVerificationFn({ data: { token } })
        if (!res.ok) return { error: res.error }
        // success → the view shows the continue CTA
      }}
    />
  )
}
