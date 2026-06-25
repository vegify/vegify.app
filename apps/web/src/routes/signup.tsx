import { createFileRoute, useRouter } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { SignupView } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { signupFn } from '../auth'

export const Route = createFileRoute('/signup')({
  component: SignupPage,
})

function SignupPage() {
  const router = useRouter()
  const queryClient = useQueryClient()
  return (
    <SignupView
      LinkComponent={LinkAdapter}
      onSubmit={async ({ name, email, password }) => {
        const res = await signupFn({ data: { name, email, password } })
        if (!res.ok) return { error: res.error }
        queryClient.clear() // start the new session with an empty content cache
        await router.invalidate()
        await router.navigate({ to: '/' })
      }}
    />
  )
}
