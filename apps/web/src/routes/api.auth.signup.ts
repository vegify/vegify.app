import { createFileRoute } from '@tanstack/react-router'

// JSON signup endpoint for the desktop (require-an-account onboarding without a trip to the web).
export const Route = createFileRoute('/api/auth/signup')({
  server: {
    handlers: {
      POST: async ({ request }) => {
        let body: { name?: string; email?: string; password?: string }
        try {
          body = await request.json()
        } catch {
          return Response.json({ error: 'Invalid request.' }, { status: 400 })
        }
        const name = (body.name ?? '').trim()
        const email = (body.email ?? '').trim()
        const password = body.password ?? ''
        if (!name || !email)
          return Response.json({ error: 'Name and email are required.' }, { status: 400 })
        if (password.length < 8)
          return Response.json({ error: 'Password must be at least 8 characters.' }, { status: 400 })
        const { createUser, createSession } = await import('@vegify/db')
        let user
        try {
          user = await createUser({ name, email, password })
        } catch {
          return Response.json(
            { error: 'An account with that email already exists.' },
            { status: 409 },
          )
        }
        const { token } = await createSession(user.id)
        return Response.json({ token, user: { id: user.id, name: user.name, email: user.email } })
      },
    },
  },
})
