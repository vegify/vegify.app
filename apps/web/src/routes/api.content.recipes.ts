import { createFileRoute } from '@tanstack/react-router'
import { withUser } from '../api-auth'
import { listRecipeCards } from '../content'

// P1 content API — recipe collection. GET = scoped list (isListed: public + own, name-sorted);
// POST = save (create or update, id in body) → { id }; DELETE ?id = delete. All Bearer-authed,
// userId stamped server-side, owner-guarded in @vegify/db. Shapes mirror the desktop DAL.
export const Route = createFileRoute('/api/content/recipes')({
  server: {
    handlers: {
      GET: ({ request }) => withUser(request, (me) => listRecipeCards(me)),
      POST: ({ request }) =>
        withUser(request, async (me) => {
          const input = await request.json()
          const { saveRecipe } = await import('@vegify/db')
          return { id: await saveRecipe({ ...input, userId: me }) }
        }),
      DELETE: ({ request }) =>
        withUser(request, async (me) => {
          const id = new URL(request.url).searchParams.get('id')
          if (!id) throw new Error('id required')
          const { deleteRecipe } = await import('@vegify/db')
          await deleteRecipe(id, me)
          return { ok: true }
        }),
    },
  },
})
