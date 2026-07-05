import { createFileRoute, notFound, redirect } from '@tanstack/react-router'
import {
  IngredientDetailPage,
  ingredientQuery,
  redirectToCanonical,
  resolveIngredientFn,
} from '../ingredient-detail'

// The `/ingredients/<segment>` segment is EITHER a slug or a legacy ULID — unlike recipes, both share
// this URL shape. This is the COMMUNAL CATALOG's canonical home; a slug that resolves to a USER-OWNED
// ingredient 301s to /<username>/ingredients/<slug> (its real canonical), as do legacy-id loads of
// owned rows. Renamed slugs 301 via slug_history.
export const Route = createFileRoute('/ingredients/$ingredientId/')({
  loader: async ({ context, params }): Promise<{ ingredientId: string }> => {
    const seg = params.ingredientId
    const hit = await resolveIngredientFn({ data: seg })
    if (hit) {
      redirectToCanonical(hit, { slug: seg }) // owned → user-scoped; renamed → current slug
      await context.queryClient.ensureQueryData(ingredientQuery(hit.ingredientId))
      return { ingredientId: hit.ingredientId }
    }
    // Not a slug → a legacy id. Load it; 301 to its canonical home when it has one.
    const ing = await context.queryClient.ensureQueryData(ingredientQuery(seg))
    if (!ing) throw notFound()
    if (ing.canonical && ing.creator) {
      throw redirect({
        to: '/$username/ingredients/$slug',
        params: { username: ing.creator, slug: ing.canonical },
        statusCode: 301, // legacy id of an OWNED ingredient → its creator-scoped canonical
      })
    }
    if (ing.canonical) {
      throw redirect({
        to: '/ingredients/$ingredientId',
        params: { ingredientId: ing.canonical },
        statusCode: 301, // legacy id → canonical catalog slug
      })
    }
    return { ingredientId: seg } // fallback: no slug yet, render by id
  },
  component: IngredientPage,
})

function IngredientPage() {
  const { ingredientId } = Route.useLoaderData() // the resolved id (slug already mapped in the loader)
  return <IngredientDetailPage ingredientId={ingredientId} />
}
