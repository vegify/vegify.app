import { createFileRoute, useRouter } from '@tanstack/react-router'
import { useQueryClient } from '@tanstack/react-query'
import { LoginView } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { loginFn } from '../auth'

export const Route = createFileRoute('/login')({
  component: LoginPage,
})

function LoginPage() {
  const router = useRouter()
  const queryClient = useQueryClient()
  return (
    <LoginView
      LinkComponent={LinkAdapter}
      onSubmit={async ({ email, password }) => {
        const res = await loginFn({ data: { email, password } })
        if (!res.ok) return { error: res.error }
        queryClient.clear() // start the new session with an empty content cache
        await router.invalidate()
        await router.navigate({ to: '/' })
      }}
    />
  )
}
