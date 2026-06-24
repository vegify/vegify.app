import { createFileRoute } from '@tanstack/react-router'
import { withUser } from '../api-auth'
import { getRecipeView } from '../content'

// Recipe detail (RecipeView, canView): public/unlisted/own, else null (desktop renders NotFound).
export const Route = createFileRoute('/api/content/recipe-detail')({
  server: {
    handlers: {
      GET: ({ request }) =>
        withUser(request, async (me) => {
          const id = new URL(request.url).searchParams.get('id')
          return id ? getRecipeView(id, me) : null
        }),
    },
  },
})
