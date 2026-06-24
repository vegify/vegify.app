import { createFileRoute } from '@tanstack/react-router'
import { withUser } from '../api-auth'
import { getRecipeEdit } from '../content'

// Recipe edit-load (RecipeEditData, isOwner): owner only, else null.
export const Route = createFileRoute('/api/content/recipe-edit')({
  server: {
    handlers: {
      GET: ({ request }) =>
        withUser(request, async (me) => {
          const id = new URL(request.url).searchParams.get('id')
          return id ? getRecipeEdit(id, me) : null
        }),
    },
  },
})
