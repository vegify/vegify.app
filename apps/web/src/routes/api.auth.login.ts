import { createFileRoute } from '@tanstack/react-router'

// JSON auth endpoint for non-browser clients (the Tauri desktop). Returns the session token in the
// body (the desktop stores it in the OS keychain) rather than only setting a cookie. A server ROUTE,
// not a server fn, so there's no Seroval wire format or CSRF token to satisfy from a native client.
export const Route = createFileRoute('/api/auth/login')({
  server: {
    handlers: {
      POST: async ({ request }) => {
        let body: { email?: string; password?: string }
        try {
          body = await request.json()
        } catch {
          return Response.json({ error: 'Invalid request.' }, { status: 400 })
        }
        const { email, password } = body
        if (!email || !password)
          return Response.json({ error: 'Email and password are required.' }, { status: 400 })
        const { authenticate, createSession } = await import('@vegify/db')
        const user = await authenticate(email, password)
        if (!user) return Response.json({ error: 'Invalid email or password.' }, { status: 401 })
        const { token } = await createSession(user.id)
        return Response.json({ token, user: { id: user.id, name: user.name, email: user.email } })
      },
    },
  },
})
