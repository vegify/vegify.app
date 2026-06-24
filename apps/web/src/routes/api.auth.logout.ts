import { createFileRoute } from '@tanstack/react-router'

// Invalidate a desktop session: the client sends `Authorization: Bearer <token>`.
export const Route = createFileRoute('/api/auth/logout')({
  server: {
    handlers: {
      POST: async ({ request }) => {
        const auth = request.headers.get('authorization') ?? ''
        const token = auth.toLowerCase().startsWith('bearer ') ? auth.slice(7) : null
        if (token) {
          const { invalidateSession } = await import('@vegify/db')
          await invalidateSession(token)
        }
        return Response.json({ ok: true })
      },
    },
  },
})
