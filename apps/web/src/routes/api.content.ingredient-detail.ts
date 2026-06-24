import { createFileRoute } from '@tanstack/react-router'
import { withUser } from '../api-auth'
import { getIngredientView } from '../content'

// Ingredient detail (IngredientEditData, canView): public/unlisted/own, else null.
export const Route = createFileRoute('/api/content/ingredient-detail')({
  server: {
    handlers: {
      GET: ({ request }) =>
        withUser(request, async (me) => {
          const id = new URL(request.url).searchParams.get('id')
          return id ? getIngredientView(id, me) : null
        }),
    },
  },
})
