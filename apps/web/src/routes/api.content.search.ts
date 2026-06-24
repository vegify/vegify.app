import { createFileRoute } from '@tanstack/react-router'
import { withUser } from '../api-auth'

// Ingredient search (IngredientSearchResult[]): public catalog + own, name-filtered, each with
// effective per-100g nutrition. Reuses @vegify/db's searchIngredients (already visibility-scoped).
export const Route = createFileRoute('/api/content/search')({
  server: {
    handlers: {
      GET: ({ request }) =>
        withUser(request, async (me) => {
          const q = new URL(request.url).searchParams.get('q') ?? ''
          const { searchIngredients } = await import('@vegify/db')
          return searchIngredients(q, me)
        }),
    },
  },
})
