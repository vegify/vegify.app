import { createFileRoute, useRouter } from '@tanstack/react-router'
import { LoginView } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { loginFn } from '../auth'

export const Route = createFileRoute('/login')({
  component: LoginPage,
})

function LoginPage() {
  const router = useRouter()
  return (
    <LoginView
      LinkComponent={LinkAdapter}
      onSubmit={async ({ email, password }) => {
        const res = await loginFn({ data: { email, password } })
        if (!res.ok) return { error: res.error }
        await router.invalidate()
        await router.navigate({ to: '/' })
      }}
    />
  )
}
