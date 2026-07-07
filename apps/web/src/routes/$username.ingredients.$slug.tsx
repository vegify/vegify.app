import { createFileRoute, notFound } from "@tanstack/react-router"

import {
  IngredientDetailPage,
  ingredientQuery,
  redirectToCanonical,
  resolveIngredientFn
} from "../ingredient-detail"

// The canonical URL for a USER-OWNED ingredient: /<username>/ingredients/<slug> — created or imported
// by that user, browsable under their profile (docs/usernames.md's model extended to ingredients).
// Wrong-owner or renamed slugs 301 to the true canonical; the communal catalog stays at
// /ingredients/<slug>.
export const Route = createFileRoute("/$username/ingredients/$slug")({
  loader: async ({ context, params }) => {
    const hit = await resolveIngredientFn({ data: params.slug })
    if (!hit) throw notFound()
    redirectToCanonical(hit, { username: params.username, slug: params.slug })
    await context.queryClient.ensureQueryData(ingredientQuery(hit.ingredientId))
    return { ingredientId: hit.ingredientId }
  },
  component: UserIngredientPage
})

function UserIngredientPage() {
  const { ingredientId } = Route.useLoaderData()
  return <IngredientDetailPage ingredientId={ingredientId} />
}
