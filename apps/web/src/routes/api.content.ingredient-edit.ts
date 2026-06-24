import { createFileRoute } from '@tanstack/react-router'
import { withUser } from '../api-auth'
import { getIngredientEdit } from '../content'

// Ingredient edit-load (IngredientEditData, isOwner): owner only, else null.
export const Route = createFileRoute('/api/content/ingredient-edit')({
  server: {
    handlers: {
      GET: ({ request }) =>
        withUser(request, async (me) => {
          const id = new URL(request.url).searchParams.get('id')
          return id ? getIngredientEdit(id, me) : null
        }),
    },
  },
})
