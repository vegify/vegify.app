import { createFileRoute } from '@tanstack/react-router'
import { withUser } from '../api-auth'
import { listIngredientCards } from '../content'

// P1 content API — ingredient collection. GET = scoped list (standalone ingredients, isListed);
// POST = save → { id }; DELETE ?id = delete. Bearer-authed, userId stamped + owner-guarded server-side.
export const Route = createFileRoute('/api/content/ingredients')({
  server: {
    handlers: {
      GET: ({ request }) => withUser(request, (me) => listIngredientCards(me)),
      POST: ({ request }) =>
        withUser(request, async (me) => {
          const input = await request.json()
          const { saveIngredient } = await import('@vegify/db')
          return { id: await saveIngredient({ ...input, userId: me }) }
        }),
      DELETE: ({ request }) =>
        withUser(request, async (me) => {
          const id = new URL(request.url).searchParams.get('id')
          if (!id) throw new Error('id required')
          const { deleteIngredient } = await import('@vegify/db')
          await deleteIngredient(id, me)
          return { ok: true }
        }),
    },
  },
})
