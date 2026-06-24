import { createFileRoute, useRouter } from '@tanstack/react-router'
import { SignupView } from '@vegify/ui'
import { LinkAdapter } from '../link'
import { signupFn } from '../auth'

export const Route = createFileRoute('/signup')({
  component: SignupPage,
})

function SignupPage() {
  const router = useRouter()
  return (
    <SignupView
      LinkComponent={LinkAdapter}
      onSubmit={async ({ name, email, password }) => {
        const res = await signupFn({ data: { name, email, password } })
        if (!res.ok) return { error: res.error }
        await router.invalidate()
        await router.navigate({ to: '/' })
      }}
    />
  )
}
