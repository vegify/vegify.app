import { createFileRoute } from '@tanstack/react-router'

// One-time owner-account bootstrap: set an INITIAL password for a pre-provisioned account that has
// none. The seeded dev@example.com predates the password column (its hash is NULL on the long-lived
// EFS DB), so it can't log in. Gated on `password_hash IS NULL` — it can only INITIALIZE a password,
// never change one — so it's inert once no null-password accounts remain (signup always sets a hash,
// and the seed now sets john's too). Remove or replace with proper reset (A5) once claimed.
export const Route = createFileRoute('/api/auth/bootstrap')({
  server: {
    handlers: {
      POST: async ({ request }) => {
        let body: { email?: string; password?: string }
        try {
          body = await request.json()
        } catch {
          return Response.json({ error: 'Invalid request.' }, { status: 400 })
        }
        const email = (body.email ?? '').trim().toLowerCase()
        const password = body.password ?? ''
        if (!email || password.length < 8)
          return Response.json(
            { error: 'Email and an 8+ character password are required.' },
            { status: 400 },
          )
        const { setInitialPassword } = await import('@vegify/db')
        const result = await setInitialPassword(email, password)
        if (!result.ok)
          return Response.json(
            { error: result.error },
            { status: result.error === 'No such account.' ? 404 : 409 },
          )
        return Response.json({ ok: true, email })
      },
    },
  },
})
